//! Deterministic text representation of a [`Netlist`] (`dv-build --netlist`).
//!
//! # Text Dump Grammar
//!
//! [`Netlist::dump`] emits one record per line in a fixed section order:
//! 1. Header: `netlist <top>`
//! 2. Primary inputs: `input <name> : <shape> = v<id>`
//! 3. Primary outputs: `output <name> : <shape> = v<id>`
//! 4. Undriven slices: `undriven <sig>[<hi>:<lo>] = v<id>`
//! 5. Memories: `memory m<id> <name> : <shape> depth <D> [<lo>..=<hi>] [init={...}]`
//!    followed by indented `  write[<i>] clk=<clk> en=v<id> addr=v<id> data=v<id>` lines
//! 6. Topological value DAG: `v<id> : <shape> = <rhs> [; <hier_name>]`
//! 7. Sequential states: `state s<id> <name> : <shape> out=v<id> next=v<id> [clk=<clk>] [async_rst=(v<id>, high|low, val=v<id>)] [latch_en=v<id>] [init=<lit>]`
//! 8. Sampled nodes: `sampled smp<id> <Kind>(v<id>, depth=<N>, clk=<clk>, gate=<vN|none>) = v<id>`
//! 9. Immediate checks: `check <assert|assume|cover> <label|-> : en=v<id> expr=v<id> clk=<clk>`
//! 10. Concurrent SVA properties: `property <kind> <label|-> clk=<sva_clk> disable=<vN|none> : <prop_expr>`
//! 11. Trace signals: `trace <name> : <shape> = v<id>`
//!
//! Shape tags have the form `<u|s><width>.<2|4>`, where `u` denotes unsigned, `s`
//! denotes two's-complement signed, `<width>` is the bit width, `.2` denotes a
//! 2-state value, and `.4` denotes a 4-state value (`u8.2`, `s16.4`). Identifiers
//! use prefixes `v<N>` for [`crate::ir::ValueId`], `s<N>` for [`crate::ir::StateId`],
//! `m<N>` for [`crate::ir::MemId`], and `smp<N>` for [`crate::ir::SampledNode`]
//! indices. Sequential clocks print as `(posedge v<N>)`, `(negedge v<N>)`, `comb`, or `none`.

use std::fmt::Write;

use sv_semantic::{
    BoundProperty, CVal, CycleBound, CycleRange, ExprArena, ExprId, ExprKind, PropExpr, PropKind, RepeatKind, SeqExpr,
    SvaClock, SvaEdge,
};

use crate::ir::{ClockSpec, NetOp, Netlist, ValueId, ValueKind};

impl Netlist {
    /// Formats this [`Netlist`] into a deterministic, line-oriented text dump for
    /// golden tests and gate diffing (`dv-build --netlist`).
    ///
    /// See the [module documentation](self) for the exact line grammar, `<u|s><W>.<2|4>`
    /// shape tags, and `vN`/`sN`/`mN`/`smpN` identifier conventions.
    pub fn dump(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "netlist {}", self.top);

        for inp in &self.inputs {
            let _ = writeln!(
                out,
                "input {} : {} = v{}",
                inp.name,
                fmt_shape(inp.width, inp.signed, inp.four_state),
                inp.value.0
            );
        }
        for o in &self.outputs {
            let _ =
                writeln!(out, "output {} : {} = v{}", o.name, fmt_shape(o.width, o.signed, o.four_state), o.value.0);
        }
        for u in &self.undriven {
            let attr =
                if u.free == crate::ir::FreeKind::Undriven { String::new() } else { format!(" ({})", u.free.as_str()) };
            let _ = writeln!(out, "undriven {}[{}:{}] = v{}{attr}", u.signal, u.hi, u.lo, u.value.0);
        }
        for m in &self.memories {
            let _ = write!(
                out,
                "memory m{} {} : {} depth {} [{}..={}]",
                m.id.0,
                m.name,
                fmt_shape(m.width, m.signed, m.four_state),
                m.depth,
                m.lo_idx,
                m.hi_idx
            );
            if !m.init.is_empty() {
                let parts: Vec<String> = m.init.iter().map(|(i, c)| format!("{i}={}", fmt_cval(c))).collect();
                let _ = write!(out, " init={{{}}}", parts.join(", "));
            }
            let _ = writeln!(out);
            for (wi, wp) in m.writes.iter().enumerate() {
                let clk_s = wp.clock.map_or_else(|| "comb".to_string(), fmt_clock);
                let _ = writeln!(
                    out,
                    "  write[{wi}] clk={clk_s} en=v{} addr=v{} data=v{}",
                    wp.enable.0, wp.addr.0, wp.data.0
                );
            }
        }

        for v in &self.values {
            let rhs = match &v.kind {
                ValueKind::Input(pi) => format!("input({})", self.inputs[*pi].name),
                ValueKind::Const(cv) => format!("const({})", fmt_cval(cv)),
                ValueKind::StateOut(sid) => format!("state_out(s{})", sid.0),
                ValueKind::MemRead { mem, addr } => format!("mem_read(m{}, v{})", mem.0, addr.0),
                ValueKind::Sampled(si) => format!("sampled(smp{si})"),
                ValueKind::Undriven(ui) => {
                    let u = &self.undriven[*ui];
                    format!("undriven({}[{}:{}])", u.signal, u.hi, u.lo)
                }
                ValueKind::Op { op, args } => fmt_op(op, args),
            };
            let name_tag = match &v.name {
                Some(nm) => format!(" ; {nm}"),
                None => String::new(),
            };
            let _ = writeln!(out, "v{} : {} = {rhs}{name_tag}", v.id.0, fmt_shape(v.width, v.signed, v.four_state));
        }

        for s in &self.states {
            let _ = write!(
                out,
                "state s{} {} : {} out=v{} next=v{}",
                s.id.0,
                s.name,
                fmt_shape(s.width, s.signed, s.four_state),
                s.out.0,
                s.next.0
            );
            if let Some(clk) = s.clock {
                let _ = write!(out, " clk={}", fmt_clock(clk));
            }
            if let Some(ar) = s.async_reset {
                let pol = if ar.active_high { "high" } else { "low" };
                let _ = write!(out, " async_rst=(v{}, {pol}, val=v{})", ar.signal.0, ar.value.0);
            }
            if let Some(le) = s.latch_enable {
                let _ = write!(out, " latch_en=v{}", le.0);
            }
            if let Some(iv) = &s.init {
                let _ = write!(out, " init={}", fmt_cval(iv));
            }
            let _ = writeln!(out);
        }

        for (si, sn) in self.sampled.iter().enumerate() {
            let clk_s = sn.clock.map_or_else(|| "none".to_string(), fmt_clock);
            let gate_s = sn.gate.map_or_else(|| "none".to_string(), |g| format!("v{}", g.0));
            let _ = writeln!(
                out,
                "sampled smp{si} {:?}(v{}, depth={}, clk={clk_s}, gate={gate_s}) = v{}",
                sn.kind, sn.expr.0, sn.depth, sn.out.0
            );
        }

        for c in &self.checks {
            let lbl = c.label.as_deref().unwrap_or("-");
            let clk_s = c.clock.map_or_else(|| "comb".to_string(), fmt_clock);
            let _ =
                writeln!(out, "check {} {lbl} : en=v{} expr=v{} clk={clk_s}", c.kind.as_str(), c.enable.0, c.expr.0);
        }

        for p in &self.properties {
            let _ = writeln!(out, "{}", fmt_bound_property(p, &self.sva_arena, &self.prop_expr_values));
        }

        for tr in &self.traces {
            let _ = writeln!(
                out,
                "trace {} : {} = v{}",
                tr.name,
                fmt_shape(tr.width, tr.signed, tr.four_state),
                tr.value.0
            );
        }

        out
    }
}

fn fmt_shape(width: u32, signed: bool, four_state: bool) -> String {
    let s = if signed { "s" } else { "u" };
    let st = if four_state { "4" } else { "2" };
    format!("{s}{width}.{st}")
}

fn fmt_clock(clk: ClockSpec) -> String {
    format!("({} v{})", clk.edge.as_str(), clk.signal.0)
}

fn fmt_cval(cv: &CVal) -> String {
    let mut bits = String::with_capacity(cv.w as usize);
    for i in (0..cv.w).rev() {
        let ch = match cv.bit_state(i) {
            sv_semantic::BitState::Zero => '0',
            sv_semantic::BitState::One => '1',
            sv_semantic::BitState::X => 'x',
            sv_semantic::BitState::Z => 'z',
        };
        bits.push(ch);
    }
    let s = if cv.s { "s" } else { "" };
    format!("{}'{s}b{bits}", cv.w)
}

fn fmt_op(op: &NetOp, args: &[ValueId]) -> String {
    let astr: Vec<String> = args.iter().map(|a| format!("v{}", a.0)).collect();
    let joined = astr.join(", ");
    match op {
        NetOp::Slice { hi, lo } => format!("slice({joined}, {hi}:{lo})"),
        NetOp::ZeroExt { width } => format!("zext({joined}, {width})"),
        NetOp::SignExt { width } => format!("sext({joined}, {width})"),
        NetOp::CastFourState { four_state } => format!("cast_4state({joined}, {four_state})"),
        NetOp::WildEq { rhs } => format!("wild_eq({joined}, {})", fmt_cval(rhs)),
        NetOp::WildNeq { rhs } => format!("wild_neq({joined}, {})", fmt_cval(rhs)),
        other => {
            let name = match other {
                NetOp::Add => "add",
                NetOp::Sub => "sub",
                NetOp::Mul => "mul",
                NetOp::UDiv => "udiv",
                NetOp::SDiv => "sdiv",
                NetOp::UMod => "umod",
                NetOp::SMod => "smod",
                NetOp::Pow => "pow",
                NetOp::And => "and",
                NetOp::Or => "or",
                NetOp::Xor => "xor",
                NetOp::Xnor => "xnor",
                NetOp::Not => "not",
                NetOp::Neg => "neg",
                NetOp::RedAnd => "red_and",
                NetOp::RedOr => "red_or",
                NetOp::RedXor => "red_xor",
                NetOp::LogNot => "log_not",
                NetOp::LogAnd => "log_and",
                NetOp::LogOr => "log_or",
                NetOp::Shl => "shl",
                NetOp::LShr => "lshr",
                NetOp::AShr => "ashr",
                NetOp::Eq => "eq",
                NetOp::Neq => "neq",
                NetOp::CaseEq => "case_eq",
                NetOp::CaseNeq => "case_neq",
                NetOp::ULt => "ult",
                NetOp::SLt => "slt",
                NetOp::ULe => "ule",
                NetOp::SLe => "sle",
                NetOp::Mux => "mux",
                NetOp::Concat => "concat",
                NetOp::CountOnes => "countones",
                NetOp::OneHot => "onehot",
                NetOp::OneHot0 => "onehot0",
                NetOp::IsUnknown => "isunknown",
                NetOp::Slice { .. }
                | NetOp::ZeroExt { .. }
                | NetOp::SignExt { .. }
                | NetOp::CastFourState { .. }
                | NetOp::WildEq { .. }
                | NetOp::WildNeq { .. } => unreachable!(),
            };
            format!("{name}({joined})")
        }
    }
}

fn fmt_bound_property(p: &BoundProperty, arena: &ExprArena, prop_vals: &[Option<ValueId>]) -> String {
    let kind_s = match p.kind {
        PropKind::Assert => "assert",
        PropKind::Assume => "assume",
        PropKind::Cover => "cover",
        PropKind::Restrict => "restrict",
    };
    let lbl = p.label.as_deref().unwrap_or("-");
    let clk_s = p.clock.as_ref().map_or_else(|| "none".to_string(), |c| fmt_sva_clock(c, arena, prop_vals));
    let dis_s = p.disable_iff.map_or_else(|| "none".to_string(), |e| fmt_sva_expr(e, arena, prop_vals));
    format!("property {kind_s} {lbl} clk={clk_s} disable={dis_s} : {}", fmt_prop_expr(&p.expr, arena, prop_vals))
}

fn fmt_sva_clock(c: &SvaClock, arena: &ExprArena, prop_vals: &[Option<ValueId>]) -> String {
    let ed = match c.edge {
        SvaEdge::Posedge => "posedge",
        SvaEdge::Negedge => "negedge",
        SvaEdge::AnyEdge => "edge",
    };
    match c.iff {
        Some(iff) => {
            format!("@({ed} {} iff {})", fmt_sva_expr(c.signal, arena, prop_vals), fmt_sva_expr(iff, arena, prop_vals))
        }
        None => format!("@({ed} {})", fmt_sva_expr(c.signal, arena, prop_vals)),
    }
}

fn fmt_range(r: CycleRange) -> String {
    match r.max {
        CycleBound::Const(m) if m == r.min => format!("{}", r.min),
        CycleBound::Const(m) => format!("{}:{}", r.min, m),
        CycleBound::Unbounded => format!("{}:$", r.min),
    }
}

fn fmt_seq_expr(s: &SeqExpr, arena: &ExprArena, prop_vals: &[Option<ValueId>]) -> String {
    match s {
        SeqExpr::Bool(e) => fmt_sva_expr(*e, arena, prop_vals),
        SeqExpr::Delay { range, lhs: None, rhs } => {
            format!("(##[{}] {})", fmt_range(*range), fmt_seq_expr(rhs, arena, prop_vals))
        }
        SeqExpr::Delay { range, lhs: Some(l), rhs } => format!(
            "({} ##[{}] {})",
            fmt_seq_expr(l, arena, prop_vals),
            fmt_range(*range),
            fmt_seq_expr(rhs, arena, prop_vals)
        ),
        SeqExpr::Fusion(l, r) => {
            format!("({} ##0 {})", fmt_seq_expr(l, arena, prop_vals), fmt_seq_expr(r, arena, prop_vals))
        }
        SeqExpr::Repeat { kind, range, seq } => {
            let op = match kind {
                RepeatKind::Consecutive => "*",
                RepeatKind::Goto => "->",
                RepeatKind::NonConsecutive => "=",
            };
            format!("({}[{op}{}])", fmt_seq_expr(seq, arena, prop_vals), fmt_range(*range))
        }
        SeqExpr::And(l, r) => {
            format!("({} and {})", fmt_seq_expr(l, arena, prop_vals), fmt_seq_expr(r, arena, prop_vals))
        }
        SeqExpr::Or(l, r) => {
            format!("({} or {})", fmt_seq_expr(l, arena, prop_vals), fmt_seq_expr(r, arena, prop_vals))
        }
        SeqExpr::Intersect(l, r) => {
            format!("({} intersect {})", fmt_seq_expr(l, arena, prop_vals), fmt_seq_expr(r, arena, prop_vals))
        }
        SeqExpr::Within(l, r) => {
            format!("({} within {})", fmt_seq_expr(l, arena, prop_vals), fmt_seq_expr(r, arena, prop_vals))
        }
        SeqExpr::Throughout { cond, seq } => {
            format!("({} throughout {})", fmt_sva_expr(*cond, arena, prop_vals), fmt_seq_expr(seq, arena, prop_vals))
        }
        SeqExpr::FirstMatch(inner) => format!("first_match({})", fmt_seq_expr(inner, arena, prop_vals)),
        SeqExpr::Clocked { clock, seq } => {
            format!("({} {})", fmt_sva_clock(clock, arena, prop_vals), fmt_seq_expr(seq, arena, prop_vals))
        }
    }
}

fn fmt_prop_expr(p: &PropExpr, arena: &ExprArena, prop_vals: &[Option<ValueId>]) -> String {
    match p {
        PropExpr::Seq(s) => fmt_seq_expr(s, arena, prop_vals),
        PropExpr::Not(x) => format!("(not {})", fmt_prop_expr(x, arena, prop_vals)),
        PropExpr::Strong(s) => format!("strong({})", fmt_seq_expr(s, arena, prop_vals)),
        PropExpr::Weak(s) => format!("weak({})", fmt_seq_expr(s, arena, prop_vals)),
        PropExpr::And(l, r) => {
            format!("({} and {})", fmt_prop_expr(l, arena, prop_vals), fmt_prop_expr(r, arena, prop_vals))
        }
        PropExpr::Or(l, r) => {
            format!("({} or {})", fmt_prop_expr(l, arena, prop_vals), fmt_prop_expr(r, arena, prop_vals))
        }
        PropExpr::Implies(l, r) => {
            format!("({} implies {})", fmt_prop_expr(l, arena, prop_vals), fmt_prop_expr(r, arena, prop_vals))
        }
        PropExpr::Iff(l, r) => {
            format!("({} iff {})", fmt_prop_expr(l, arena, prop_vals), fmt_prop_expr(r, arena, prop_vals))
        }
        PropExpr::OverlappedImplication { lhs, rhs } => {
            format!("({} |-> {})", fmt_seq_expr(lhs, arena, prop_vals), fmt_prop_expr(rhs, arena, prop_vals))
        }
        PropExpr::NonOverlappedImplication { lhs, rhs } => {
            format!("({} |=> {})", fmt_seq_expr(lhs, arena, prop_vals), fmt_prop_expr(rhs, arena, prop_vals))
        }
        PropExpr::FollowedBy { overlapped, lhs, rhs } => {
            let op = if *overlapped { "#-#" } else { "#=#" };
            format!("({} {op} {})", fmt_seq_expr(lhs, arena, prop_vals), fmt_prop_expr(rhs, arena, prop_vals))
        }
        PropExpr::Nexttime { strong, cycles, prop } => {
            let kw = if *strong { "s_nexttime" } else { "nexttime" };
            format!("({kw}[{cycles}] {})", fmt_prop_expr(prop, arena, prop_vals))
        }
        PropExpr::Always { strong, range, prop } => {
            let kw = if *strong { "s_always" } else { "always" };
            format!("({kw}[{}] {})", fmt_range(*range), fmt_prop_expr(prop, arena, prop_vals))
        }
        PropExpr::Eventually { strong, range, prop } => {
            let kw = if *strong { "s_eventually" } else { "eventually" };
            format!("({kw}[{}] {})", fmt_range(*range), fmt_prop_expr(prop, arena, prop_vals))
        }
        PropExpr::Until { strong, overlapping, lhs, rhs } => {
            let kw = match (*strong, *overlapping) {
                (false, false) => "until",
                (true, false) => "s_until",
                (false, true) => "until_with",
                (true, true) => "s_until_with",
            };
            format!("({} {kw} {})", fmt_prop_expr(lhs, arena, prop_vals), fmt_prop_expr(rhs, arena, prop_vals))
        }
        PropExpr::IfElse { cond, then_prop, else_prop } => match else_prop {
            Some(ep) => format!(
                "(if ({}) {} else {})",
                fmt_sva_expr(*cond, arena, prop_vals),
                fmt_prop_expr(then_prop, arena, prop_vals),
                fmt_prop_expr(ep, arena, prop_vals)
            ),
            None => format!(
                "(if ({}) {})",
                fmt_sva_expr(*cond, arena, prop_vals),
                fmt_prop_expr(then_prop, arena, prop_vals)
            ),
        },
        PropExpr::AcceptOn { sync, reject, cond, prop } => {
            let kw = match (*sync, *reject) {
                (false, false) => "accept_on",
                (false, true) => "reject_on",
                (true, false) => "sync_accept_on",
                (true, true) => "sync_reject_on",
            };
            format!("({kw}({}) {})", fmt_sva_expr(*cond, arena, prop_vals), fmt_prop_expr(prop, arena, prop_vals))
        }
        PropExpr::Clocked { clock, prop } => {
            format!("({} {})", fmt_sva_clock(clock, arena, prop_vals), fmt_prop_expr(prop, arena, prop_vals))
        }
    }
}

fn fmt_sva_expr(id: ExprId, arena: &ExprArena, prop_vals: &[Option<ValueId>]) -> String {
    if let Some(Some(vid)) = prop_vals.get(id.0 as usize) {
        return format!("v{}", vid.0);
    }
    let e = arena.get(id);
    if let Some(cv) = &e.cval {
        return fmt_cval(cv);
    }
    match &e.kind {
        ExprKind::Symbol(sid) => format!("sym#{}", sid.0),
        ExprKind::Local(lid) => format!("loc#{}", lid.0),
        ExprKind::Const(cv) => fmt_cval(cv),
        ExprKind::Unary { op, arg } => format!("({op:?} {})", fmt_sva_expr(*arg, arena, prop_vals)),
        ExprKind::Binary { op, lhs, rhs } => {
            format!("({} {op:?} {})", fmt_sva_expr(*lhs, arena, prop_vals), fmt_sva_expr(*rhs, arena, prop_vals))
        }
        ExprKind::Resize { arg, width, signed } => {
            let fn_nm = if *signed { "sext" } else { "zext" };
            format!("{fn_nm}({}, {width})", fmt_sva_expr(*arg, arena, prop_vals))
        }
        ExprKind::CastSign { arg, .. } | ExprKind::CastFourState { arg, .. } => fmt_sva_expr(*arg, arena, prop_vals),
        ExprKind::Sampled { kind, expr, depth, .. } => {
            format!("{kind:?}({}, {depth})", fmt_sva_expr(*expr, arena, prop_vals))
        }
        _ => format!("expr#{}", id.0),
    }
}
