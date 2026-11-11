//! Bit-interval driver resolution for flattened signals (IEEE 1800-2023 §6.5 / §9.2.2).
//!
//! Supports disjoint bit-slice / struct-member drivers across separate `always_ff` blocks,
//! `assign` statements, or child module output ports (`multi_driver_ff`, `multi_driver_struct`),
//! rejects overlapping bit drivers fail-closed naming the exact `[hi:lo]` bit slice and both
//! source [`Loc`]s (`overlap_drivers`), and emits [`UndrivenSlice`] nodes for undriven bit ranges.

use sv_semantic::{CVal, Loc};

use crate::builder::{BuildKind, NetBuilder};
use crate::ir::{NetOp, UndrivenSlice, ValueId};

/// Origin category of a bit-slice driver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DriverKind {
    /// Top-level or hierarchical input port connection.
    InputPort,
    /// Child module output port connection.
    ChildOutput,
    /// `assign lhs = rhs;` or `alias` / gate output.
    ContinuousAssign,
    /// `always_comb` or `always @*` procedural block.
    AlwaysComb,
    /// `always_latch` procedural block.
    AlwaysLatch,
    /// Clocked `always_ff` or `always @(posedge/negedge ...)` block.
    AlwaysFf,
}

impl DriverKind {
    /// Human-readable description for multi-driver diagnostics.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            DriverKind::InputPort => "input port",
            DriverKind::ChildOutput => "instance output port",
            DriverKind::ContinuousAssign => "continuous assignment",
            DriverKind::AlwaysComb => "always_comb block",
            DriverKind::AlwaysLatch => "always_latch block",
            DriverKind::AlwaysFf => "clocked procedural block",
        }
    }

    /// `true` for procedural drivers (`always_ff`, `always_comb`, `always_latch`).
    pub(crate) const fn is_procedural(self) -> bool {
        matches!(self, DriverKind::AlwaysComb | DriverKind::AlwaysLatch | DriverKind::AlwaysFf)
    }
}

/// One driven contiguous physical bit interval `[lo ..= hi]` (`0 <= lo <= hi < width`).
#[derive(Clone, Debug)]
pub(crate) struct BitSliceDriver {
    /// Inclusive physical LSB offset (`0`-based).
    pub(crate) lo: u32,
    /// Inclusive physical MSB offset (`0`-based, `width == hi - lo + 1`).
    pub(crate) hi: u32,
    /// Driven value of width `hi - lo + 1`.
    pub(crate) value: ValueId,
    /// Driver category.
    pub(crate) kind: DriverKind,
    /// Source location of this driver.
    pub(crate) loc: Loc,
}

/// Per-signal driver accumulator for a flattened packed signal or scalarized struct member.
#[derive(Clone, Debug)]
pub(crate) struct SignalSlot {
    /// Full hierarchical name (`"top.u0.q"` or `"top.pkt.valid"`).
    pub(crate) name: String,
    /// Bit width (`>= 1`).
    pub(crate) width: u32,
    /// Signedness.
    pub(crate) signed: bool,
    /// Four-state flag.
    pub(crate) four_state: bool,
    /// `true` when declared as a net (`wire`, `tri`, ...) rather than a variable (`logic`, `reg`, ...).
    pub(crate) is_net: bool,
    /// Pre-allocated placeholder [`ValueId`] read by combinational/sequential consumers.
    pub(crate) placeholder: ValueId,
    /// Optional constant initial value (`logic q = 1'b1` or `initial q = 1'b1`).
    pub(crate) init: Option<CVal>,
    /// Declaration source location.
    pub(crate) loc: Loc,
    /// Registered bit-interval drivers across all blocks/ports.
    pub(crate) drivers: Vec<BitSliceDriver>,
}

impl SignalSlot {
    /// Appends a bit-interval driver `[lo ..= hi]`.
    pub(crate) fn add_driver(&mut self, lo: u32, hi: u32, value: ValueId, kind: DriverKind, loc: Loc) {
        debug_assert!(lo <= hi && hi < self.width);
        self.drivers.push(BitSliceDriver { lo, hi, value, kind, loc });
    }
}

/// Resolves bit-interval drivers for every [`SignalSlot`], checking for illegal overlaps,
/// creating [`UndrivenSlice`]s for undriven bit gaps, and binding `slot.placeholder`.
pub(crate) fn resolve_all_signals(slots: &[SignalSlot], builder: &mut NetBuilder) {
    for slot in slots {
        resolve_one_signal(slot, builder);
    }
}

fn resolve_one_signal(slot: &SignalSlot, builder: &mut NetBuilder) {
    // Net legality: procedural blocks cannot drive a `wire` net.
    if slot.is_net {
        for d in &slot.drivers {
            if d.kind.is_procedural() {
                builder.err(d.loc, format!("procedural {} cannot drive net '{}'", d.kind.as_str(), slot.name));
                return;
            }
        }
    }

    // Check pairwise bit-interval overlaps (`O(k^2)` where `k` is the number of slice drivers on one signal, typically 1..8).
    let mut had_overlap = false;
    for i in 0..slot.drivers.len() {
        for j in (i + 1)..slot.drivers.len() {
            let a = &slot.drivers[i];
            let b = &slot.drivers[j];
            if a.lo <= b.hi && b.lo <= a.hi {
                had_overlap = true;
                let ov_lo = a.lo.max(b.lo);
                let ov_hi = a.hi.min(b.hi);
                let bits = if slot.width == 1 {
                    String::new()
                } else if ov_lo == ov_hi {
                    format!("[{ov_lo}]")
                } else {
                    format!("[{ov_hi}:{ov_lo}]")
                };
                let first_file = builder.file_of(a.loc);
                builder.err(
                    b.loc,
                    format!(
                        "overlapping drivers on '{}{bits}': driven by {} and {} (first driven at {}:{}:{})",
                        slot.name,
                        b.kind.as_str(),
                        a.kind.as_str(),
                        first_file,
                        a.loc.line,
                        a.loc.column
                    ),
                );
            }
        }
    }
    if had_overlap {
        return;
    }

    // Fast path: zero drivers.
    if slot.drivers.is_empty() {
        if let Some(init_cv) = &slot.init {
            // Initialized variable with no runtime reassignment (`logic [3:0] cfg = 4'd9;`).
            let c = builder.alloc_const(init_cv.cast(slot.width, slot.signed), slot.four_state, slot.loc);
            builder.bind_placeholder(slot.placeholder, c);
        } else {
            let uid = builder.undriven.len();
            let uval = builder.alloc_raw(
                slot.width,
                slot.signed,
                slot.four_state,
                BuildKind::Undriven(uid),
                Some(slot.name.clone()),
                slot.loc,
            );
            builder.undriven.push(UndrivenSlice {
                signal: slot.name.clone(),
                hi: slot.width - 1,
                lo: 0,
                value: uval,
                loc: slot.loc,
            });
            builder.bind_placeholder(slot.placeholder, uval);
        }
        return;
    }

    // Fast path: single full-width driver.
    if slot.drivers.len() == 1 && slot.drivers[0].lo == 0 && slot.drivers[0].hi + 1 == slot.width {
        let dval = builder.resize_val(slot.drivers[0].value, slot.width, false, slot.signed, slot.drivers[0].loc);
        builder.bind_placeholder(slot.placeholder, dval);
        return;
    }

    // General path: sort disjoint slice drivers by ascending `lo`, fill gaps with `init` or `UndrivenSlice`,
    // and concatenate MSB-first (`parts.reverse()`) into `slot.placeholder`!
    let mut sorted = slot.drivers.clone();
    sorted.sort_by_key(|d| d.lo);

    let mut lsb_to_msb_parts: Vec<ValueId> = Vec::new();
    let mut cursor = 0u32;
    for d in &sorted {
        if cursor < d.lo {
            lsb_to_msb_parts.push(make_gap_slice(slot, cursor, d.lo - 1, builder));
        }
        let dw = (d.hi - d.lo) + 1;
        let part = builder.resize_val(d.value, dw, false, false, d.loc);
        lsb_to_msb_parts.push(part);
        cursor = d.hi + 1;
    }
    if cursor < slot.width {
        lsb_to_msb_parts.push(make_gap_slice(slot, cursor, slot.width - 1, builder));
    }

    lsb_to_msb_parts.reverse();
    let full = builder.alloc_op(NetOp::Concat, lsb_to_msb_parts, slot.width, slot.signed, slot.four_state, slot.loc);
    builder.bind_placeholder(slot.placeholder, full);
}

fn make_gap_slice(slot: &SignalSlot, lo: u32, hi: u32, builder: &mut NetBuilder) -> ValueId {
    let w = (hi - lo) + 1;
    if let Some(init_cv) = &slot.init {
        return builder.alloc_const(init_cv.slice(lo, w, false), slot.four_state, slot.loc);
    }
    let uid = builder.undriven.len();
    let name = format!("{}[{hi}:{lo}]", slot.name);
    let uval = builder.alloc_raw(w, false, slot.four_state, BuildKind::Undriven(uid), Some(name), slot.loc);
    builder.undriven.push(UndrivenSlice { signal: slot.name.clone(), hi, lo, value: uval, loc: slot.loc });
    uval
}
