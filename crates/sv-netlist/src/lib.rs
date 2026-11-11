//! Pure-Rust IEEE 1800-2023 RTL synthesis and formal lowering into a word-level netlist IR (`svlang::netlist`).
//!
//! Lowering transforms parsed CST [`sv_syntax::Tree`]s and an [`sv_semantic::ElaboratedDesign`]
//! into a format-neutral, topologically sorted word-level [`Netlist`] plus bound SVA
//! [`sv_semantic::BoundProperty`] trees for Btor2 or AIGER model checkers, equivalence checkers,
//! RTL linters, and cycle simulators.
//!
//! # Key Contracts
//!
//! - **Fail-closed [`LowerOutcome`].** [`lower_design`] and [`lower_trees`] never drop unsupported
//!   SystemVerilog constructs silently:
//!   - [`LowerOutcome::Lowered`] returns a complete, combinational-cycle-free [`Netlist`].
//!   - [`LowerOutcome::Unsupported`] reports legal SystemVerilog constructs outside the synthesizable
//!     or formal RTL subset (such as dynamic unpacked types, unbounded loops, switch-level gates, or
//!     4-state case-equality operators when [`LowerConfig::allow_four_state_ops`] is `false`).
//!   - [`LowerOutcome::Error`] reports invalid RTL, elaboration errors, combinational feedback loops,
//!     inferred latches in `always_comb`, or illegal multiple drivers.
//! - **Physical `0`-based LSB bit offsets.** Every packed signal, struct, array, and [`NetOp::Slice`]
//!   is normalized to physical `0`-based LSB coordinates (`0` is the least-significant bit and
//!   `width - 1` is the most-significant bit), regardless of whether the SystemVerilog declaration
//!   used descending (`[7:0]`), ascending (`[0:7]`), or non-zero (`[15:8]`) bounds.
//! - **2-state vs 4-state semantics.** By default ([`LowerConfig::allow_four_state_ops`] = `false`),
//!   lowering targets standard 2-state word-level solvers: `===`, `!==`, and `$isunknown` are
//!   rejected with [`LowerOutcome::Unsupported`], wildcard equality (`==?`, `!=?`, `casez`, `casex`)
//!   treats constant mask bits statically, and dynamic index out-of-bounds guards coerce cleanly to
//!   2-state booleans while tracking [`ValueNode::four_state`] provenance on reads. Enabling
//!   [`LowerConfig::allow_four_state_ops`] preserves explicit [`NetOp::CaseEq`], [`NetOp::CaseNeq`],
//!   and [`NetOp::IsUnknown`] nodes for 4-state back ends.
//!
//! # Example
//!
//! ```
//! use sv_netlist::{lower_trees, LowerConfig, LowerOutcome};
//! use sv_syntax::{parse_source, Options};
//!
//! let tree = parse_source(
//!     "module accum (input logic clk, rst, input logic [7:0] d, output logic [7:0] q);\n\
//!        always_ff @(posedge clk) if (rst) q <= 8'd0; else q <= q + d;\n\
//!        assert property (@(posedge clk) disable iff (rst) q >= $past(q) || d != 8'd0);\n\
//!      endmodule\n",
//!     "accum.sv",
//!     Options::default(),
//! );
//!
//! let nl = match lower_trees(&[tree], &[], &[], &LowerConfig::default()) {
//!     LowerOutcome::Lowered(nl) => nl,
//!     other => panic!("unexpected outcome: {other:?}"),
//! };
//! assert_eq!(nl.top, "accum");
//! assert_eq!(nl.states.len(), 1);
//! assert_eq!(nl.properties.len(), 1);
//! ```

mod builder;
mod drivers;
pub mod dump;
mod expr_lower;
pub mod ir;
mod loops_sub;
mod lower;
mod proc;

pub use ir::*;
pub use lower::lower_design;

/// Runs syntax, semantic, and elaboration checks on `trees` and lowers the elaborated design
/// into a [`Netlist`] fail-closed.
///
/// Top-module selection uses `tops` when non-empty, falling back to [`LowerConfig::top`] when
/// `tops` is empty, and finally to automatic uninstantiated-module inference when a single top
/// module exists. When multiple top modules are elaborated and [`LowerConfig::top`] is `None`,
/// lowering returns [`LowerOutcome::Error`].
///
/// `params` applies `-G name=value` parameter overrides during elaboration.
pub fn lower_trees(
    trees: &[sv_syntax::Tree],
    tops: &[String],
    params: &[(String, String)],
    config: &LowerConfig,
) -> LowerOutcome {
    let mut loc = sv_semantic::Locator::default();
    let mut errs = Vec::new();
    for (ix, t) in trees.iter().enumerate() {
        for d in &t.pp_diagnostics {
            let (f, line, column) = loc.position(ix, t, d.origin);
            let file = loc.files().get(f as usize).cloned().unwrap_or_default();
            errs.push(NetlistDiag {
                loc: sv_semantic::Loc { file: f, line, column, expansion: None },
                file,
                message: d.message.clone(),
                unsupported: false,
            });
        }
        for e in &t.errors {
            let tok = (e.token as usize).min(t.tokens.len().saturating_sub(1)) as u32;
            let l = if t.tokens.is_empty() { sv_semantic::Loc::default() } else { loc.loc(ix, t, tok) };
            let file = loc.files().get(l.file as usize).cloned().unwrap_or_default();
            errs.push(NetlistDiag { loc: l, file, message: e.message.clone(), unsupported: false });
        }
    }
    let effective_tops: Vec<String> =
        if tops.is_empty() { config.top.iter().cloned().collect() } else { tops.to_vec() };
    let (sema, elab) = sv_semantic::semantic_diags_with_params(trees, &effective_tops, params);
    for d in sema {
        let t = &trees[d.tree];
        let l = t.token_bounds(d.node).map(|(tok, _)| loc.loc(d.tree, t, tok)).unwrap_or_default();
        let file = loc.files().get(l.file as usize).cloned().unwrap_or_default();
        errs.push(NetlistDiag { loc: l, file, message: d.message, unsupported: false });
    }
    if !errs.is_empty() {
        return LowerOutcome::Error(errs);
    }
    lower_design(trees, &elab.design, config)
}
