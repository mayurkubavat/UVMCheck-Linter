//! Symbolic procedural execution for `always_ff`, `always_comb`, `always_latch`,
//! `always @(...)`, and `always @*` blocks (IEEE 1800-2023 Clauses 9, 10, 12).

use sv_semantic::fx::FxMap;
use sv_semantic::{
    BinaryOp, CVal, ElabItem, ExprArena, ExprBinder, ExprId, ExprKind, InstanceId, Int, Loc, LocalBinding, LocalId,
    PortDir, RtlType, SymbolId, SymbolKind, UnaryOp,
};
use sv_syntax::{Child, NodeId, SyntaxKind, Tree};

use crate::builder::{BuildKind, NetBuilder};
use crate::drivers::DriverKind;
use crate::expr_lower::{
    addr_bits as proc_addr_bits, coerce_two_state_en, compute_packed_slice_offset as proc_packed_offset,
    compute_unpacked_index_bounds as proc_unpacked_bounds, extract_lsb_bit0, lower_dynamic_packed_slice,
    make_oob_obj_val, mux_obj_val, replace_const_packed_slice, rmw_dynamic_packed_slice, ExprLowerer, ObjSlot, ObjVal,
};
use crate::ir::{AsyncReset, Check, CheckKind, ClockSpec, Edge, MemWritePort, NetOp, State, StateId, ValueId};
use crate::lower::{own_first_tok, subtree_preorder, DesignLowerer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProcKind {
    Clocked,
    AlwaysComb,
    AlwaysLatch,
    LegacyComb,
    Subroutine,
}

#[derive(Clone, Debug)]
pub(crate) struct SlotWriteState {
    /// Current blocking-read value for this slot inside the procedural block.
    pub cur_val: ValueId,
    /// Staged next-state / combinational output value for this slot.
    pub next_val: ValueId,
    /// Per-bit flag: `true` if any statement in this block ever wrote bit `b`.
    pub ever_written: Vec<bool>,
    /// Per-bit syntactic definite-assignment flag (for `always_comb` latch checking).
    pub def_written: Vec<bool>,
    /// Per-bit runtime write-enable condition (for `always_latch` enable extraction).
    pub write_cond: Vec<ValueId>,
    /// Source location of the first write to this slot in the block.
    pub first_loc: Loc,
    /// Whether any blocking (`=`) assignment targeted this slot in the current clocked block.
    pub has_blocking: bool,
    /// Whether any non-blocking (`<=`) assignment targeted this slot in the current clocked block.
    pub has_nba: bool,
}

#[derive(Clone)]
pub(crate) struct ProcCtx {
    pub inst_id: InstanceId,
    pub scope_prefix: String,
    pub gen_consts: Vec<(String, CVal)>,
    pub kind: ProcKind,
    pub active_clock: Option<ClockSpec>,
    pub path_cond: ValueId,
    pub returned_cond: ValueId,
    pub break_cond: ValueId,
    pub continue_cond: ValueId,
    pub cur_syms: FxMap<SymbolId, ObjVal>,
    pub cur_locals: FxMap<LocalId, ObjVal>,
    pub local_decls: FxMap<String, LocalBinding>,
    pub local_typedefs: FxMap<String, RtlType>,
    pub slot_writes: FxMap<usize, SlotWriteState>,
    pub ret_local: Option<(LocalId, RtlType)>,
    pub def_gated: bool,
    pub unconditional_lids: Vec<LocalId>,
    pub in_initial: bool,
    pub loop_depth: u32,
    pub mem_writes_start: Vec<usize>,
}

impl ProcCtx {
    pub fn new(
        builder: &mut NetBuilder,
        inst_id: InstanceId,
        scope_prefix: String,
        gen_consts: Vec<(String, CVal)>,
        kind: ProcKind,
        active_clock: Option<ClockSpec>,
        loc: Loc,
    ) -> Self {
        let one = builder.alloc_const(CVal::new(1, false, 1), false, loc);
        let zero = builder.alloc_const(CVal::new(1, false, 0), false, loc);
        let mem_writes_start = builder.memories.iter().map(|m| m.writes.len()).collect();
        Self {
            inst_id,
            scope_prefix,
            gen_consts,
            kind,
            active_clock,
            path_cond: one,
            returned_cond: zero,
            break_cond: zero,
            continue_cond: zero,
            cur_syms: FxMap::default(),
            cur_locals: FxMap::default(),
            local_decls: FxMap::default(),
            local_typedefs: FxMap::default(),
            slot_writes: FxMap::default(),
            ret_local: None,
            def_gated: false,
            unconditional_lids: Vec::new(),
            in_initial: false,
            loop_depth: 0,
            mem_writes_start,
        }
    }

    pub fn jump_is_clean(&self, builder: &NetBuilder) -> bool {
        if self.def_gated {
            return false;
        }
        for &c in &[self.returned_cond, self.break_cond, self.continue_cond] {
            let is_zero = builder.const_cval(c).is_some_and(|cv| cv.known() && !cv.is_true());
            if !is_zero {
                return false;
            }
        }
        true
    }

    pub fn local_reach(&self, builder: &mut NetBuilder, loc: Loc) -> ValueId {
        let not_ret = builder.alloc_not1(self.returned_cond, loc);
        let not_brk = builder.alloc_not1(self.break_cond, loc);
        let not_cnt = builder.alloc_not1(self.continue_cond, loc);
        let a = builder.alloc_and1(not_ret, not_brk, loc);
        builder.alloc_and1(a, not_cnt, loc)
    }

    pub fn effective_reach(&self, builder: &mut NetBuilder, loc: Loc) -> ValueId {
        let lr = self.local_reach(builder, loc);
        builder.alloc_and1(self.path_cond, lr, loc)
    }

    pub fn restore_scope(
        &mut self,
        builder: &NetBuilder,
        mut saved_decls: FxMap<String, LocalBinding>,
        saved_tdefs: FxMap<String, RtlType>,
    ) {
        for lb in saved_decls.values_mut() {
            if let Some(ov) = self.cur_locals.get(&lb.id) {
                lb.cval = ov.as_packed().and_then(|v| builder.const_cval(v).cloned());
            }
        }
        self.local_decls = saved_decls;
        self.local_typedefs = saved_tdefs;
    }

    pub fn is_statically_unreachable(&self, builder: &NetBuilder) -> bool {
        if let Some(cv) = builder.const_cval(self.path_cond) {
            if cv.known() && !cv.is_true() {
                return true;
            }
        }
        for &c in &[self.returned_cond, self.break_cond, self.continue_cond] {
            if let Some(cv) = builder.const_cval(c) {
                if cv.known() && cv.is_true() {
                    return true;
                }
            }
        }
        false
    }
}

/// Projection step from a root `SymbolId` or `LocalId` on an assignment LHS.
#[derive(Clone, Debug)]
enum LhsStep {
    StructField(String),
    PackedConstSlice {
        lo: u32,
        width: u32,
        slice_total_w: u32,
        rhs_lo: u32,
    },
    PackedDynSlice {
        in_bounds: ValueId,
        bit_lo: ValueId,
        slice_w: u32,
    },
    PackedIndexedSlice {
        idx_val: ValueId,
        idx_signed: bool,
        bounds: (i128, i128),
        elem_w: u32,
        count: u32,
        ascending: bool,
    },
    ArrayElem {
        idx_val: ValueId,
        idx_signed: bool,
        dim: (i128, i128),
        elem_ty: RtlType,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ResetTarget {
    Symbol(SymbolId),
    Member { base: Box<ResetTarget>, field: String },
    Bit { base: Box<ResetTarget>, index: i128 },
}

enum ConstCasePat {
    Exact { val: CVal, cw: u32, cs: bool },
    Wild { val: CVal, wild_mask: Int, cw: u32, cs: bool },
    Range { lo: CVal, hi: CVal, cw: u32, cs: bool },
}

impl ConstCasePat {
    fn matches_two_state(&self, sv: u32, dom_w: u32, dom_s: bool) -> bool {
        match self {
            ConstCasePat::Exact { val, cw, cs } => {
                if !val.known() {
                    return false;
                }
                let sv_ext = CVal::new(dom_w, dom_s && *cs, sv as i128).cast(*cw, *cs);
                sv_ext.bits_v() == val.bits_v()
            }
            ConstCasePat::Wild { val, wild_mask, cw, cs } => {
                let sv_ext = CVal::new(dom_w, dom_s && *cs, sv as i128).cast(*cw, *cs);
                let keep = wild_mask.not().trunc((*cw).max(1), false);
                sv_ext.bits_v().and(&keep) == val.bits_v().and(&keep)
            }
            ConstCasePat::Range { lo, hi, cw, cs } => {
                if !lo.known() || !hi.known() {
                    return false;
                }
                let sv_ext = CVal::new(dom_w, dom_s && *cs, sv as i128).cast(*cw, *cs);
                lo.v <= sv_ext.v && sv_ext.v <= hi.v
            }
        }
    }
}

fn cval_z_mask(cv: &CVal) -> Int {
    cv.u.and(&cv.bits_v()).trunc(cv.w.max(1), false)
}

fn cval_x_only_mask(cv: &CVal) -> Int {
    cv.u.and(&cv.bits_v().not()).trunc(cv.w.max(1), false)
}

#[derive(Clone, Debug)]
enum LhsRoot {
    Symbol(SymbolId),
    Local(LocalId, RtlType),
}

impl<'a> DesignLowerer<'a> {
    pub(crate) fn lower_procedural_block(&mut self, inst_id: InstanceId, item: &ElabItem) {
        let t = &self.trees[item.tree];
        let kw = own_first_tok(t, item.node).unwrap_or("");
        match kw {
            "initial" | "final" => {}
            "always_comb" => {
                let Some(body) = t.children(item.node).find(|&c| t.kind(c) != SyntaxKind::Attribute) else { return };
                self.run_comb_or_latch_block(inst_id, item, body, ProcKind::AlwaysComb);
            }
            "always_latch" => {
                let Some(body) = t.children(item.node).find(|&c| t.kind(c) != SyntaxKind::Attribute) else { return };
                self.run_comb_or_latch_block(inst_id, item, body, ProcKind::AlwaysLatch);
            }
            "always_ff" => {
                let Some(timed) = t.child(item.node, SyntaxKind::TimedStmt) else {
                    self.builder.err(item.loc, "always_ff requires an edge-triggered @(posedge/negedge ...) control");
                    return;
                };
                self.run_clocked_block(inst_id, item, timed, true);
            }
            "always" => {
                let Some(child) = t.children(item.node).find(|&c| t.kind(c) != SyntaxKind::Attribute) else { return };
                if t.kind(child) != SyntaxKind::TimedStmt {
                    self.builder.unsup(item.loc, "always block without sensitivity list is not synthesizable");
                    return;
                }
                let Some(tc) = t.child(child, SyntaxKind::TimingControl) else { return };
                let events: Vec<NodeId> = t.children(tc).filter(|&c| t.kind(c) == SyntaxKind::EventExpr).collect();
                let is_star = events.is_empty() && t.bare_text(tc).contains('*');
                let all_level = !events.is_empty()
                    && events.iter().all(|&ev| {
                        let inner = t.child(ev, SyntaxKind::EventExpr).unwrap_or(ev);
                        !has_edge_kw(t, inner)
                    });
                if is_star || all_level {
                    // Verify every signal in explicit sensitivity list `@(a or b)` resolves fail-closed!
                    for ev in &events {
                        let inner = t.child(*ev, SyntaxKind::EventExpr).unwrap_or(*ev);
                        if let Some(sig) = t.children(inner).find(|&c| t.kind(c) != SyntaxKind::Attribute) {
                            let mut arena = ExprArena::new();
                            let mut binder = ExprBinder::new(
                                self.trees,
                                self.design,
                                inst_id,
                                &item.scope_prefix,
                                &item.gen_consts,
                                &mut arena,
                            );
                            let _ = binder.bind_self(item.tree, sig);
                            self.flush_expr_diags(binder.diags);
                        }
                    }
                    let Some(body) = t
                        .children(child)
                        .find(|&c| !matches!(t.kind(c), SyntaxKind::Attribute | SyntaxKind::TimingControl))
                    else {
                        return;
                    };
                    self.run_comb_or_latch_block(inst_id, item, body, ProcKind::LegacyComb);
                } else {
                    self.run_clocked_block(inst_id, item, child, false);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn lower_module_immediate_assert(&mut self, inst_id: InstanceId, item: &ElabItem) {
        let mut ctx = ProcCtx::new(
            &mut self.builder,
            inst_id,
            item.scope_prefix.clone(),
            item.gen_consts.clone(),
            ProcKind::AlwaysComb,
            None,
            item.loc,
        );
        self.exec_stmt(item.tree, item.node, &mut ctx);
    }

    fn run_comb_or_latch_block(&mut self, inst_id: InstanceId, item: &ElabItem, body: NodeId, kind: ProcKind) {
        if self.has_embedded_timing_control(item.tree, body) {
            self.builder.unsup(item.loc, "intra-block timing control ('#', '@', 'wait') is not synthesizable");
            return;
        }
        let mut ctx = ProcCtx::new(
            &mut self.builder,
            inst_id,
            item.scope_prefix.clone(),
            item.gen_consts.clone(),
            kind,
            None,
            item.loc,
        );
        self.exec_stmt(item.tree, body, &mut ctx);

        // Emit combinational or latch drivers for every written bit interval!
        let mut sidxs: Vec<usize> = ctx.slot_writes.keys().copied().collect();
        sidxs.sort_unstable();
        for sidx in sidxs {
            let sw = &ctx.slot_writes[&sidx];
            let runs = contiguous_runs(&sw.ever_written);
            for (lo, hi) in runs {
                let slice_w = (hi - lo) + 1;
                let any_incomplete = (lo..=hi).any(|b| !sw.def_written[b as usize]);
                if kind == ProcKind::AlwaysComb && any_incomplete {
                    let bits = if slice_w == self.slots[sidx].width {
                        String::new()
                    } else if lo == hi {
                        format!("[{lo}]")
                    } else {
                        format!("[{hi}:{lo}]")
                    };
                    self.builder.err(
                        sw.first_loc,
                        format!(
                            "inferred latch for '{}{bits}' in always_comb block (not assigned on all paths)",
                            self.slots[sidx].name
                        ),
                    );
                    continue;
                }
                if kind == ProcKind::AlwaysLatch || (kind == ProcKind::LegacyComb && any_incomplete) {
                    // Create a transparent latch `State` (`clock: None`, `latch_enable: Some(en)`).
                    let mut en = sw.write_cond[lo as usize];
                    for b in (lo + 1)..=hi {
                        en = self.builder.alloc_or1(en, sw.write_cond[b as usize], sw.first_loc);
                    }
                    let state_id = StateId(self.builder.states.len() as u32);
                    let s_signed = lo == 0 && slice_w == self.slots[sidx].width && self.slots[sidx].signed;
                    let s_four = self.slots[sidx].four_state;
                    let state_name = if lo == 0 && slice_w == self.slots[sidx].width {
                        self.slots[sidx].name.clone()
                    } else {
                        format!("{}[{hi}:{lo}]", self.slots[sidx].name)
                    };
                    let out_val = self.builder.alloc_raw(
                        slice_w,
                        s_signed,
                        s_four,
                        BuildKind::StateOut(state_id),
                        Some(state_name.clone()),
                        sw.first_loc,
                    );
                    let data_slice = self.builder.alloc_op(
                        NetOp::Slice { hi, lo },
                        vec![sw.next_val],
                        slice_w,
                        s_signed,
                        s_four,
                        sw.first_loc,
                    );
                    let init_cv = self.slots[sidx].init.as_ref().map(|c| c.slice(lo, slice_w, s_signed));
                    self.builder.states.push(State {
                        id: state_id,
                        name: state_name,
                        width: slice_w,
                        signed: s_signed,
                        four_state: s_four,
                        clock: None,
                        async_reset: None,
                        latch_enable: Some(en),
                        init: init_cv,
                        next: data_slice,
                        out: out_val,
                        loc: sw.first_loc,
                    });
                    self.slots[sidx].add_driver(lo, hi, out_val, DriverKind::AlwaysLatch, sw.first_loc);
                } else {
                    let s_signed = lo == 0 && slice_w == self.slots[sidx].width && self.slots[sidx].signed;
                    let s_four = self.slots[sidx].four_state;
                    let val = self.builder.alloc_op(
                        NetOp::Slice { hi, lo },
                        vec![sw.next_val],
                        slice_w,
                        s_signed,
                        s_four,
                        sw.first_loc,
                    );
                    self.slots[sidx].add_driver(lo, hi, val, DriverKind::AlwaysComb, sw.first_loc);
                }
            }
        }
    }

    fn run_clocked_block(&mut self, inst_id: InstanceId, item: &ElabItem, timed: NodeId, is_always_ff: bool) {
        let t = &self.trees[item.tree];
        let Some(tc) = t.child(timed, SyntaxKind::TimingControl) else { return };
        let Some(body) =
            t.children(timed).find(|&c| !matches!(t.kind(c), SyntaxKind::Attribute | SyntaxKind::TimingControl))
        else {
            return;
        };
        if self.has_embedded_timing_control(item.tree, body) {
            self.builder.unsup(
                item.loc,
                "intra-block timing control ('#', '@', 'wait') inside clocked block is not synthesizable",
            );
            return;
        }

        let events: Vec<NodeId> = t.children(tc).filter(|&c| t.kind(c) == SyntaxKind::EventExpr).collect();
        if events.is_empty() {
            self.builder.err(item.loc, "missing clock event in sensitivity list");
            return;
        }
        let mut parsed_events: Vec<(Edge, ValueId, Option<ResetTarget>, Loc)> = Vec::new();
        for ev in events {
            let inner = t.child(ev, SyntaxKind::EventExpr).unwrap_or(ev);
            let edge_opt = event_edge(t, inner);
            let Some(edge) = edge_opt else {
                self.builder.err(item.loc, "clocked procedural block event must specify 'posedge' or 'negedge'");
                return;
            };
            if t.node(inner)
                .children
                .iter()
                .any(|c| matches!(*c, Child::Token(i) if &*t.tokens[i as usize].text == "iff"))
            {
                self.builder.unsup(item.loc, "clocking event 'iff' qualifier on procedural block is not supported");
                return;
            }
            let Some(sig_node) = t.children(inner).find(|&c| t.kind(c) != SyntaxKind::Attribute) else {
                continue;
            };
            let mut arena = ExprArena::new();
            let mut binder =
                ExprBinder::new(self.trees, self.design, inst_id, &item.scope_prefix, &item.gen_consts, &mut arena);
            let ev_loc = binder.loc_of(item.tree, inner);
            let Some(eid) = binder.bind_self(item.tree, sig_node) else {
                self.flush_expr_diags(binder.diags);
                return;
            };
            self.flush_expr_diags(binder.diags);
            let root_sym = extract_root_symbol(&arena, eid);
            let Some(sig_val) = self.eval_comb_expr(&arena, eid).and_then(|o| o.as_packed()) else {
                return;
            };
            let lsb_sig = extract_lsb_bit0(&mut self.builder, sig_val, ev_loc);
            parsed_events.push((edge, lsb_sig, root_sym, ev_loc));
        }

        if parsed_events.len() > 2 {
            self.builder
                .unsup(item.loc, "clocked procedural block with more than 2 sensitivity events is not synthesizable");
            return;
        }

        // Single-event synchronous block: `@(posedge clk)` / `@(negedge clk)`.
        if parsed_events.len() == 1 {
            let (edge, clk_sig, _, _) = parsed_events[0];
            let clk = ClockSpec { signal: clk_sig, edge };
            let mut ctx = ProcCtx::new(
                &mut self.builder,
                inst_id,
                item.scope_prefix.clone(),
                item.gen_consts.clone(),
                ProcKind::Clocked,
                Some(clk),
                item.loc,
            );
            self.exec_stmt(item.tree, body, &mut ctx);
            self.finalize_clocked_states(&ctx.slot_writes, None, clk);
            return;
        }

        // Two-event async-reset block: `@(posedge clk or posedge/negedge rst) if (rst_cond) ... else ...`
        let _ = is_always_ff;
        let Some(top_if) = unwrap_single_if_stmt(t, body) else {
            self.builder
                .err(item.loc, "two-event clocked block must have top-level 'if (reset) ... else ...' structure");
            return;
        };
        let if_kids: Vec<NodeId> = t.children(top_if).filter(|&c| t.kind(c) != SyntaxKind::Attribute).collect();
        let Some(&cond_node) = if_kids.first() else { return };
        let Some(&rst_stmt) = if_kids.get(1) else { return };
        let clk_stmt_opt = top_if_else_stmt(t, top_if, &if_kids);

        let mut cond_arena = ExprArena::new();
        let mut cond_binder =
            ExprBinder::new(self.trees, self.design, inst_id, &item.scope_prefix, &item.gen_consts, &mut cond_arena);
        let cond_loc = cond_binder.loc_of(item.tree, cond_node);
        let Some(cond_eid) = cond_binder.bind_condition(item.tree, cond_node) else {
            self.flush_expr_diags(cond_binder.diags);
            return;
        };
        self.flush_expr_diags(cond_binder.diags);

        let Some((rst_idx, active_high)) = match_async_reset_event(&cond_arena, cond_eid, &parsed_events) else {
            self.builder.err(
                cond_loc,
                "asynchronous reset 'if' condition does not match sensitivity list signal or edge polarity",
            );
            return;
        };
        let clk_idx = 1 - rst_idx;
        let (clk_edge, clk_sig, _, _) = parsed_events[clk_idx];
        let (_, rst_sig, _, _) = parsed_events[rst_idx];
        let clk = ClockSpec { signal: clk_sig, edge: clk_edge };

        let rst_asserted = if active_high { rst_sig } else { self.builder.alloc_not1(rst_sig, cond_loc) };
        let rst_deasserted = self.builder.alloc_not1(rst_asserted, cond_loc);

        // Execute reset branch to gather asynchronous reset values and gate immediate checks.
        let mut rst_ctx = ProcCtx::new(
            &mut self.builder,
            inst_id,
            item.scope_prefix.clone(),
            item.gen_consts.clone(),
            ProcKind::Clocked,
            Some(clk),
            cond_loc,
        );
        let rst_mem_marks: Vec<usize> = self.builder.memories.iter().map(|m| m.writes.len()).collect();
        let rst_check_mark = self.builder.checks.len();
        self.exec_stmt(item.tree, rst_stmt, &mut rst_ctx);
        for (mi, &old_len) in rst_mem_marks.iter().enumerate() {
            if self.builder.memories[mi].writes.len() > old_len {
                let wloc = self.builder.memories[mi].writes[old_len].loc;
                self.builder.memories[mi].writes.truncate(old_len);
                self.builder.unsup(wloc, "memory write inside asynchronous reset branch is not supported");
            }
        }
        for ci in rst_check_mark..self.builder.checks.len() {
            let old_en = self.builder.checks[ci].enable;
            let cloc = self.builder.checks[ci].loc;
            self.builder.checks[ci].enable = self.builder.alloc_and1(rst_asserted, old_en, cloc);
        }

        // Execute synchronous `else` branch, guarding memory writes and checks by `rst_deasserted`.
        let mut sync_ctx = ProcCtx::new(
            &mut self.builder,
            inst_id,
            item.scope_prefix.clone(),
            item.gen_consts.clone(),
            ProcKind::Clocked,
            Some(clk),
            item.loc,
        );
        let mem_write_marks: Vec<usize> = self.builder.memories.iter().map(|m| m.writes.len()).collect();
        let check_mark = self.builder.checks.len();
        if let Some(clk_stmt) = clk_stmt_opt {
            self.exec_stmt(item.tree, clk_stmt, &mut sync_ctx);
        }
        for (mi, &old_len) in mem_write_marks.iter().enumerate() {
            let writes_len = self.builder.memories[mi].writes.len();
            for wi in old_len..writes_len {
                let old_en = self.builder.memories[mi].writes[wi].enable;
                let wloc = self.builder.memories[mi].writes[wi].loc;
                let gated_en = self.builder.alloc_and1(rst_deasserted, old_en, wloc);
                let two_state_en = coerce_two_state_en(&mut self.builder, gated_en, wloc);
                self.builder.memories[mi].writes[wi].enable = two_state_en;
            }
        }
        for ci in check_mark..self.builder.checks.len() {
            let old_en = self.builder.checks[ci].enable;
            let cloc = self.builder.checks[ci].loc;
            self.builder.checks[ci].enable = self.builder.alloc_and1(rst_deasserted, old_en, cloc);
        }

        self.finalize_clocked_states(
            &sync_ctx.slot_writes,
            Some((rst_sig, active_high, rst_asserted, &rst_ctx.slot_writes)),
            clk,
        );
    }

    fn finalize_clocked_states(
        &mut self,
        sync_writes: &FxMap<usize, SlotWriteState>,
        async_rst: Option<(ValueId, bool, ValueId, &FxMap<usize, SlotWriteState>)>,
        clk: ClockSpec,
    ) {
        let mut all_sidxs: Vec<usize> = sync_writes.keys().copied().collect();
        if let Some((_, _, _, rw)) = async_rst {
            for &k in rw.keys() {
                if !all_sidxs.contains(&k) {
                    all_sidxs.push(k);
                }
            }
        }
        all_sidxs.sort_unstable();

        for sidx in all_sidxs {
            let w = self.slots[sidx].width as usize;
            let mut combined_ever = vec![false; w];
            let mut loc = self.slots[sidx].loc;
            let mut any_blk = false;
            let mut any_nba = false;
            let mut already_mixed_in_single_branch = false;
            if let Some(sw) = sync_writes.get(&sidx) {
                loc = sw.first_loc;
                any_blk |= sw.has_blocking;
                any_nba |= sw.has_nba;
                already_mixed_in_single_branch |= sw.has_blocking && sw.has_nba;
                for (i, &b) in sw.ever_written.iter().enumerate() {
                    combined_ever[i] |= b;
                }
            }
            if let Some((_, _, _, rw_map)) = async_rst {
                if let Some(rw) = rw_map.get(&sidx) {
                    if !sync_writes.contains_key(&sidx) {
                        loc = rw.first_loc;
                    }
                    any_blk |= rw.has_blocking;
                    any_nba |= rw.has_nba;
                    already_mixed_in_single_branch |= rw.has_blocking && rw.has_nba;
                    for (i, &b) in rw.ever_written.iter().enumerate() {
                        combined_ever[i] |= b;
                    }
                }
            }
            if any_blk && any_nba && !already_mixed_in_single_branch {
                self.builder.err(
                    loc,
                    format!(
                        "mixing blocking ('=') and non-blocking ('<=') assignments to '{}' in one clocked block is illegal",
                        self.slots[sidx].name
                    ),
                );
            }

            // Split runs whenever async-reset presence changes across bits of the same variable!
            let rst_mask: Vec<bool> = match async_rst {
                Some((_, _, _, rw_map)) => {
                    rw_map.get(&sidx).map(|rw| rw.ever_written.clone()).unwrap_or_else(|| vec![false; w])
                }
                None => vec![false; w],
            };
            let runs = contiguous_runs_with_tag(&combined_ever, &rst_mask);
            for (lo, hi, has_rst) in runs {
                let slice_w = (hi - lo) + 1;
                let s_signed = lo == 0 && slice_w == self.slots[sidx].width && self.slots[sidx].signed;
                let s_four = self.slots[sidx].four_state;
                let state_id = StateId(self.builder.states.len() as u32);
                let state_name = if lo == 0 && slice_w == self.slots[sidx].width {
                    self.slots[sidx].name.clone()
                } else {
                    format!("{}[{hi}:{lo}]", self.slots[sidx].name)
                };
                let out_val = self.builder.alloc_raw(
                    slice_w,
                    s_signed,
                    s_four,
                    BuildKind::StateOut(state_id),
                    Some(state_name.clone()),
                    loc,
                );

                let raw_sync_next = match sync_writes.get(&sidx) {
                    Some(sw) => self.builder.alloc_op(
                        NetOp::Slice { hi, lo },
                        vec![sw.next_val],
                        slice_w,
                        s_signed,
                        s_four,
                        loc,
                    ),
                    None => out_val,
                };

                let (ar_spec, final_next) = match async_rst {
                    Some((rst_sig, active_high, _rst_asserted, rw_map)) if has_rst => {
                        let rw = &rw_map[&sidx];
                        let rst_val = self.builder.alloc_op(
                            NetOp::Slice { hi, lo },
                            vec![rw.next_val],
                            slice_w,
                            s_signed,
                            s_four,
                            rw.first_loc,
                        );
                        (Some(AsyncReset { signal: rst_sig, active_high, value: rst_val }), raw_sync_next)
                    }
                    Some((_, _, rst_asserted, _)) => {
                        // Unreset register inside `else` of `if (rst)`: hold `out_val` when `rst_asserted`!
                        let held = self.builder.alloc_op(
                            NetOp::Mux,
                            vec![rst_asserted, out_val, raw_sync_next],
                            slice_w,
                            s_signed,
                            s_four,
                            loc,
                        );
                        (None, held)
                    }
                    None => (None, raw_sync_next),
                };

                let init_cv = self.slots[sidx].init.as_ref().map(|c| c.slice(lo, slice_w, s_signed));
                self.builder.states.push(State {
                    id: state_id,
                    name: state_name,
                    width: slice_w,
                    signed: s_signed,
                    four_state: s_four,
                    clock: Some(clk),
                    async_reset: ar_spec,
                    latch_enable: None,
                    init: init_cv,
                    next: final_next,
                    out: out_val,
                    loc,
                });
                self.slots[sidx].add_driver(lo, hi, out_val, DriverKind::AlwaysFf, loc);
            }
        }
    }

    pub(crate) fn exec_stmt(&mut self, tree_idx: usize, stmt: NodeId, ctx: &mut ProcCtx) {
        if ctx.is_statically_unreachable(&self.builder) {
            return;
        }
        let t = &self.trees[tree_idx];
        if ctx.in_initial {
            match t.kind(stmt) {
                SyntaxKind::EmptyStmt | SyntaxKind::ImportDecl => {}
                SyntaxKind::SeqBlock => {
                    for ch in t.children(stmt) {
                        if !matches!(t.kind(ch), SyntaxKind::Attribute | SyntaxKind::StmtLabel | SyntaxKind::Name) {
                            self.exec_stmt(tree_idx, ch, ctx);
                        }
                    }
                }
                SyntaxKind::IfStmt => {
                    self.exec_if_stmt(tree_idx, stmt, ctx);
                }
                SyntaxKind::ExprStmt => {
                    if let Some(expr_node) =
                        t.children(stmt).find(|&c| !matches!(t.kind(c), SyntaxKind::Attribute | SyntaxKind::StmtLabel))
                    {
                        self.exec_expr_stmt(tree_idx, expr_node, ctx);
                    }
                }
                other => {
                    let loc = self.stmt_loc(tree_idx, stmt, ctx);
                    self.builder.unsup(loc, format!("unsupported statement '{other:?}' inside initial block"));
                }
            }
            return;
        }
        match t.kind(stmt) {
            SyntaxKind::EmptyStmt | SyntaxKind::ImportDecl => {}
            SyntaxKind::ConcurrentAssert => {
                let loc = self.stmt_loc(tree_idx, stmt, ctx);
                let reach = ctx.effective_reach(&mut self.builder, loc);
                if !self.call_stack.is_empty() || ctx.loop_depth > 0 || !self.is_const_one(reach) {
                    self.builder.unsup(
                        loc,
                        "procedural concurrent assertion inside subroutine, loop, or conditional branch is not supported",
                    );
                }
            }
            SyntaxKind::SeqBlock => {
                let saved_decls = ctx.local_decls.clone();
                let saved_tdefs = ctx.local_typedefs.clone();
                for ch in t.children(stmt) {
                    if !matches!(t.kind(ch), SyntaxKind::Attribute | SyntaxKind::StmtLabel | SyntaxKind::Name) {
                        self.exec_stmt(tree_idx, ch, ctx);
                    }
                }
                ctx.restore_scope(&self.builder, saved_decls, saved_tdefs);
            }
            SyntaxKind::DataDecl | SyntaxKind::ParamDecl => {
                self.exec_local_decl(tree_idx, stmt, ctx);
            }
            SyntaxKind::TypedefDecl => {
                self.exec_local_typedef(tree_idx, stmt, ctx);
            }
            SyntaxKind::IfStmt => {
                self.exec_if_stmt(tree_idx, stmt, ctx);
            }
            SyntaxKind::CaseStmt => {
                self.exec_case_stmt(tree_idx, stmt, ctx);
            }
            SyntaxKind::LoopStmt | SyntaxKind::DoWhileStmt => {
                self.exec_loop_stmt(tree_idx, stmt, ctx);
            }
            SyntaxKind::AssertStmt => {
                self.exec_immediate_assert(tree_idx, stmt, ctx);
            }
            SyntaxKind::ReturnStmt => {
                self.exec_return_stmt(tree_idx, stmt, ctx);
            }
            SyntaxKind::JumpStmt => {
                let loc = self.stmt_loc(tree_idx, stmt, ctx);
                let reach = ctx.local_reach(&mut self.builder, loc);
                let kw = own_first_tok(t, stmt).unwrap_or("");
                if kw == "break" {
                    ctx.break_cond = self.builder.alloc_or1(ctx.break_cond, reach, loc);
                } else if kw == "continue" {
                    ctx.continue_cond = self.builder.alloc_or1(ctx.continue_cond, reach, loc);
                }
            }
            SyntaxKind::ExprStmt => {
                if let Some(expr_node) =
                    t.children(stmt).find(|&c| !matches!(t.kind(c), SyntaxKind::Attribute | SyntaxKind::StmtLabel))
                {
                    self.exec_expr_stmt(tree_idx, expr_node, ctx);
                }
            }
            other => {
                let loc = self.stmt_loc(tree_idx, stmt, ctx);
                self.builder.unsup(loc, format!("unsupported procedural statement '{other:?}' in RTL"));
            }
        }
    }

    fn exec_if_stmt(&mut self, tree_idx: usize, stmt: NodeId, ctx: &mut ProcCtx) {
        let t = &self.trees[tree_idx];
        let kids: Vec<NodeId> =
            t.children(stmt).filter(|&c| !matches!(t.kind(c), SyntaxKind::Attribute | SyntaxKind::StmtLabel)).collect();
        let Some(&cond_node) = kids.first() else { return };
        let Some(&then_stmt) = kids.get(1) else { return };
        let else_stmt = top_if_else_stmt(t, stmt, &kids);

        let loc = self.stmt_loc(tree_idx, cond_node, ctx);
        let Some(cond_val) = self.eval_proc_cond(tree_idx, cond_node, ctx) else { return };

        // Constant-condition fast path: take only the active branch.
        if let Some(cc) = self.builder.const_cval(cond_val).cloned() {
            if cc.known() {
                if cc.is_true() {
                    self.exec_stmt(tree_idx, then_stmt, ctx);
                } else if let Some(es) = else_stmt {
                    self.exec_stmt(tree_idx, es, ctx);
                }
                return;
            }
        }
        if ctx.in_initial {
            self.builder.unsup(loc, "non-constant 'if' condition inside initial block is not supported");
            return;
        }

        let reach = ctx.effective_reach(&mut self.builder, loc);
        let then_path = self.builder.alloc_and1(reach, cond_val, loc);
        let not_cond = self.builder.alloc_not1(cond_val, loc);
        let else_path = self.builder.alloc_and1(reach, not_cond, loc);

        let mut then_ctx = ctx.clone();
        then_ctx.path_cond = then_path;
        self.exec_stmt(tree_idx, then_stmt, &mut then_ctx);

        let mut else_ctx = ctx.clone();
        else_ctx.path_cond = else_path;
        if let Some(es) = else_stmt {
            self.exec_stmt(tree_idx, es, &mut else_ctx);
        }

        let saved_path_cond = ctx.path_cond;
        let mut merged = self.merge_branch_ctxs(cond_val, then_ctx, else_ctx, else_stmt.is_some(), loc);
        merged.path_cond = saved_path_cond;
        *ctx = merged;
    }

    fn exec_case_stmt(&mut self, tree_idx: usize, stmt: NodeId, ctx: &mut ProcCtx) {
        let t = &self.trees[tree_idx];
        let own = own_toks(t, stmt);
        if own.contains(&"randcase") || own.contains(&"matches") {
            let loc = self.stmt_loc(tree_idx, stmt, ctx);
            self.builder.unsup(loc, "randcase / pattern-matching case statement is not supported in RTL");
            return;
        }
        let is_inside = own.contains(&"inside");
        let is_casez = own.contains(&"casez");
        let is_casex = own.contains(&"casex");

        let Some((Some(sel_node), branches, default_stmt)) = parse_case_branches(t, stmt) else {
            return;
        };
        let loc = self.stmt_loc(tree_idx, sel_node, ctx);

        // Bind selector (in IEEE 1800 section 12.5 table-wide context when not `case inside`).
        let (sel_arena, sel_eid, case_w, case_signed, dom_w, dom_s) = if is_inside {
            let mut sa = ExprArena::new();
            let Some(se) = self.bind_in_proc(tree_idx, sel_node, ctx, &mut sa, false) else { return };
            let Some((sw, ss, _)) = sa.get(se).ty.integral_shape() else {
                self.builder.err(loc, "case selector expression must be integral or packed");
                return;
            };
            (sa, se, sw, ss, sw, ss)
        } else {
            let mut peek_arena = ExprArena::new();
            let mut peek_binder = self.make_proc_binder(ctx, &mut peek_arena);
            let d0 = peek_binder.diags.len();
            let sel_peek = peek_binder.peek_self_shape(tree_idx, sel_node);
            if peek_binder.diags.len() > d0 || sel_peek.is_none() {
                self.flush_expr_diags(peek_binder.diags);
                let mut sa = ExprArena::new();
                if self.bind_in_proc(tree_idx, sel_node, ctx, &mut sa, false).is_some() {
                    self.builder.err(loc, "case selector expression must be integral or packed");
                }
                return;
            }
            let (sw, ss, _) = sel_peek.expect("checked Some");
            let mut cw = sw;
            let mut cs = ss;
            for (pats, _) in &branches {
                for &pat_node in pats {
                    if let Some((pw, ps, _)) = peek_binder.peek_self_shape(tree_idx, pat_node) {
                        cw = cw.max(pw);
                        cs = cs && ps;
                    }
                }
            }
            let mut sa = ExprArena::new();
            let mut sb = self.make_proc_binder(ctx, &mut sa);
            let Some(se) = sb.bind_with_context(tree_idx, sel_node, cw, cs) else {
                self.flush_expr_diags(sb.diags);
                return;
            };
            self.flush_expr_diags(sb.diags);
            let mut root_e = se;
            while let ExprKind::Resize { arg, .. } | ExprKind::CastFourState { arg, .. } = sa.get(root_e).kind {
                root_e = arg;
            }
            let (dw, ds) = if sa.get(root_e).ty.bit_width() == Some(sw) { (sw, ss) } else { (cw, cs) };
            (sa, se, cw, cs, dw, ds)
        };
        let sel_ty = sel_arena.get(sel_eid).ty.clone();
        let sel_cval = sel_arena.get(sel_eid).cval.clone();
        let Some(sel_val) = self.lower_in_proc(&sel_arena, sel_eid, ctx).and_then(|o| o.as_packed()) else { return };

        let base_ctx = ctx.clone();
        let orig_reach = ctx.effective_reach(&mut self.builder, loc);
        let mut unmatched = self.builder.alloc_const(CVal::new(1, false, 1), false, loc);
        let mut any_constant_exhaustive = false;
        let mut const_pats: Vec<ConstCasePat> = Vec::new();

        let mut arm_defs: Vec<FxMap<usize, Vec<bool>>> = Vec::new();
        let mut evaluated_arms: Vec<(ValueId, ProcCtx)> = Vec::with_capacity(branches.len());

        for (pats, arm_stmt) in &branches {
            let mut arm_hit = self.builder.alloc_const(CVal::new(1, false, 0), false, loc);
            for &pat_node in pats {
                if let Some((hit, cp_opt)) = self.eval_case_item_match(
                    tree_idx,
                    pat_node,
                    sel_val,
                    &sel_ty,
                    sel_cval.as_ref(),
                    case_w,
                    case_signed,
                    is_inside,
                    is_casez,
                    is_casex,
                    &base_ctx,
                    loc,
                ) {
                    if let Some(cp) = cp_opt {
                        const_pats.push(cp);
                    }
                    arm_hit = self.builder.alloc_or1(arm_hit, hit, loc);
                }
            }
            let take_arm = self.builder.alloc_and1(unmatched, arm_hit, loc);
            let arm_path = self.builder.alloc_and1(orig_reach, take_arm, loc);

            let mut arm_ctx = base_ctx.clone();
            arm_ctx.path_cond = arm_path;
            if !arm_ctx.is_statically_unreachable(&self.builder) {
                self.exec_stmt(tree_idx, *arm_stmt, &mut arm_ctx);
                arm_defs.push(arm_ctx.slot_writes.iter().map(|(&k, v)| (k, v.def_written.clone())).collect());
                evaluated_arms.push((arm_hit, arm_ctx));
                let not_hit = self.builder.alloc_not1(arm_hit, loc);
                unmatched = self.builder.alloc_and1(unmatched, not_hit, loc);
            }
        }

        if default_stmt.is_none() && dom_w <= 10 && !const_pats.is_empty() {
            let total_states = 1u32 << dom_w;
            any_constant_exhaustive =
                (0..total_states).all(|sv| const_pats.iter().any(|cp| cp.matches_two_state(sv, dom_w, dom_s)));
        }

        let def_ctx_opt = if let Some(def_s) = default_stmt {
            let def_path = self.builder.alloc_and1(orig_reach, unmatched, loc);
            let mut def_ctx = base_ctx.clone();
            def_ctx.path_cond = def_path;
            if !def_ctx.is_statically_unreachable(&self.builder) {
                self.exec_stmt(tree_idx, def_s, &mut def_ctx);
                arm_defs.push(def_ctx.slot_writes.iter().map(|(&k, v)| (k, v.def_written.clone())).collect());
                Some(def_ctx)
            } else {
                None
            }
        } else {
            None
        };

        let mut accum_ctx = if let Some(def_ctx) = def_ctx_opt {
            let mut acc = def_ctx;
            for (arm_hit, arm_ctx) in evaluated_arms.into_iter().rev() {
                acc = self.merge_branch_ctxs(arm_hit, arm_ctx, acc, true, loc);
            }
            acc
        } else if (default_stmt.is_some() || any_constant_exhaustive) && !evaluated_arms.is_empty() {
            let (_, mut acc) = evaluated_arms.pop().expect("non-empty");
            for (arm_hit, arm_ctx) in evaluated_arms.into_iter().rev() {
                acc = self.merge_branch_ctxs(arm_hit, arm_ctx, acc, true, loc);
            }
            acc
        } else {
            let mut acc = base_ctx.clone();
            for (arm_hit, arm_ctx) in evaluated_arms.into_iter().rev() {
                acc = self.merge_branch_ctxs(arm_hit, arm_ctx, acc, false, loc);
            }
            acc
        };

        // Definite-assignment tracking across `case`: bit `b` is definitely assigned iff it was
        // already definitely assigned before `case`, OR (`covers_all` AND every arm assigned it).
        let covers_all = (default_stmt.is_some() || any_constant_exhaustive) && !arm_defs.is_empty();
        for (&sidx, sw) in &mut accum_ctx.slot_writes {
            let pre_def = base_ctx
                .slot_writes
                .get(&sidx)
                .map(|p| p.def_written.clone())
                .unwrap_or_else(|| vec![false; sw.def_written.len()]);
            for (b, out_d) in sw.def_written.iter_mut().enumerate() {
                let all_arms = covers_all && arm_defs.iter().all(|m| m.get(&sidx).is_some_and(|v| v[b]));
                *out_d = pre_def[b] || all_arms;
            }
        }

        accum_ctx.path_cond = ctx.path_cond;
        *ctx = accum_ctx;
    }

    #[allow(clippy::too_many_arguments)]
    fn eval_case_item_match(
        &mut self,
        tree_idx: usize,
        pat_node: NodeId,
        sel_val: ValueId,
        sel_ty: &RtlType,
        sel_cval: Option<&CVal>,
        case_w: u32,
        case_signed: bool,
        is_inside: bool,
        is_casez: bool,
        is_casex: bool,
        ctx: &ProcCtx,
        loc: Loc,
    ) -> Option<(ValueId, Option<ConstCasePat>)> {
        let t = &self.trees[tree_idx];
        let (sw, ss, sf) = sel_ty.integral_shape()?;

        if is_inside && t.kind(pat_node) == SyntaxKind::OpenRange {
            let kids: Vec<NodeId> = t.children(pat_node).filter(|&c| t.kind(c) != SyntaxKind::Attribute).collect();
            if kids.len() == 2 {
                let (cw, cs) = {
                    let mut peek_arena = ExprArena::new();
                    let mut pb = self.make_proc_binder(ctx, &mut peek_arena);
                    let lsh = pb.peek_self_shape(tree_idx, kids[0]).unwrap_or((sw, ss, sf));
                    let hsh = pb.peek_self_shape(tree_idx, kids[1]).unwrap_or((sw, ss, sf));
                    (sw.max(lsh.0).max(hsh.0), ss && lsh.1 && hsh.1)
                };
                let mut arena = ExprArena::new();
                let (lo_e, hi_e) = {
                    let mut binder = self.make_proc_binder(ctx, &mut arena);
                    let le = binder.bind_with_context(tree_idx, kids[0], cw, cs);
                    let he = binder.bind_with_context(tree_idx, kids[1], cw, cs);
                    self.flush_expr_diags(binder.diags);
                    (le?, he?)
                };
                let ls = arena.get(lo_e).ty.is_signed();
                let lf = arena.get(lo_e).ty.is_four_state();
                let hs = arena.get(hi_e).ty.is_signed();
                let hf = arena.get(hi_e).ty.is_four_state();
                let lo_cval = arena.get(lo_e).cval.clone();
                let hi_cval = arena.get(hi_e).cval.clone();
                let lo_v = self.lower_in_proc(&arena, lo_e, ctx)?.as_packed()?;
                let hi_v = self.lower_in_proc(&arena, hi_e, ctx)?.as_packed()?;
                let eff_cs = ss && ls && hs;
                let f4 = sf || lf || hf;
                let s_ext = self.builder.resize_val(sel_val, cw, eff_cs, eff_cs, loc);
                let lo_ext = self.builder.resize_val(lo_v, cw, eff_cs, eff_cs, loc);
                let hi_ext = self.builder.resize_val(hi_v, cw, eff_cs, eff_cs, loc);
                let cmp = if eff_cs { NetOp::SLe } else { NetOp::ULe };
                let ge = self.builder.alloc_op(cmp.clone(), vec![lo_ext, s_ext], 1, false, f4, loc);
                let le = self.builder.alloc_op(cmp, vec![s_ext, hi_ext], 1, false, f4, loc);
                let cp = match (lo_cval, hi_cval) {
                    (Some(lc), Some(hc)) if lc.known() && hc.known() => {
                        Some(ConstCasePat::Range { lo: lc.cast(cw, eff_cs), hi: hc.cast(cw, eff_cs), cw, cs: eff_cs })
                    }
                    _ => None,
                };
                return Some((self.builder.alloc_and1(ge, le, loc), cp));
            }
        }

        let (cw, cs) = if is_inside {
            let mut peek_arena = ExprArena::new();
            let mut pb = self.make_proc_binder(ctx, &mut peek_arena);
            let psh = pb.peek_self_shape(tree_idx, pat_node).unwrap_or((sw, ss, sf));
            (sw.max(psh.0), ss && psh.1)
        } else {
            (case_w, case_signed)
        };

        let mut arena = ExprArena::new();
        let pat_e = {
            let mut binder = self.make_proc_binder(ctx, &mut arena);
            let r = binder.bind_with_context(tree_idx, pat_node, cw, cs);
            self.flush_expr_diags(binder.diags);
            r?
        };
        let pat_loc = arena.get(pat_e).loc;
        let ps = arena.get(pat_e).ty.is_signed();
        let pf = arena.get(pat_e).ty.is_four_state();
        let eff_cs = ss && ps;
        let pat_cval = arena.get(pat_e).cval.clone().map(|c| c.cast(cw, eff_cs));
        let sel_cval_ext = sel_cval.map(|c| c.cast(cw, eff_cs));

        if (is_casez || is_casex) && !is_inside && self.config.allow_four_state_ops && pat_cval.is_none() {
            self.builder
                .unsup(pat_loc, "non-constant casez/casex item is not supported when allow_four_state_ops is enabled");
            return None;
        }

        // `case (...) inside`: asymmetric wildcard equality (`==?`), only RHS `x`/`z`/`?` are wildcards.
        if is_inside {
            if let Some(pat) = &pat_cval {
                if !pat.known() {
                    let s_ext = self.builder.resize_val(sel_val, cw, eff_cs, eff_cs, loc);
                    let hit = self.builder.alloc_op(NetOp::WildEq { rhs: pat.clone() }, vec![s_ext], 1, false, sf, loc);
                    let cp = ConstCasePat::Wild { val: pat.clone(), wild_mask: pat.u.clone(), cw, cs: eff_cs };
                    return Some((hit, Some(cp)));
                }
            }
            let pat_v = self.lower_in_proc(&arena, pat_e, ctx)?.as_packed()?;
            let s_ext = self.builder.resize_val(sel_val, cw, eff_cs, eff_cs, loc);
            let p_ext = self.builder.resize_val(pat_v, cw, eff_cs, eff_cs, loc);
            let hit = self.builder.alloc_op(NetOp::Eq, vec![s_ext, p_ext], 1, false, sf || pf, loc);
            let cp = pat_cval.filter(|c| c.known()).map(|val| ConstCasePat::Exact { val, cw, cs: eff_cs });
            return Some((hit, cp));
        }

        // `casez`: ONLY `z`/`?` bits (`cval_z_mask`) are wildcards, symmetric across constant `sel` and `pat`.
        if is_casez && !is_casex {
            let pat_z = pat_cval.as_ref().map(cval_z_mask).unwrap_or_else(|| Int::from(0));
            let sel_z = sel_cval_ext.as_ref().map(cval_z_mask).unwrap_or_else(|| Int::from(0));
            let wild_z = pat_z.or(&sel_z).trunc(cw.max(1), false);
            let keep_mask = wild_z.not().trunc(cw.max(1), false);
            let pat_x_nonwild =
                pat_cval.as_ref().map(|p| cval_x_only_mask(p).and(&keep_mask)).unwrap_or_else(|| Int::from(0));
            let sel_x_nonwild =
                sel_cval_ext.as_ref().map(|s| cval_x_only_mask(s).and(&keep_mask)).unwrap_or_else(|| Int::from(0));
            let has_nonwild_x = !pat_x_nonwild.is_zero() || !sel_x_nonwild.is_zero();

            if !self.config.allow_four_state_ops {
                if has_nonwild_x {
                    let zero = self.builder.alloc_const(CVal::new(1, false, 0), false, loc);
                    return Some((zero, None));
                }
                if !wild_z.is_zero() {
                    if let Some(pat) = &pat_cval {
                        let s_ext = self.builder.resize_val(sel_val, cw, eff_cs, eff_cs, loc);
                        let eff_pat = CVal::four(cw, eff_cs, pat.bits_v().or(&wild_z), wild_z.clone());
                        let hit = self.builder.alloc_op(NetOp::WildEq { rhs: eff_pat }, vec![s_ext], 1, false, sf, loc);
                        let cp = ConstCasePat::Wild { val: pat.clone(), wild_mask: wild_z, cw, cs: eff_cs };
                        return Some((hit, Some(cp)));
                    }
                    let pat_v = self.lower_in_proc(&arena, pat_e, ctx)?.as_packed()?;
                    let p_ext = self.builder.resize_val(pat_v, cw, eff_cs, eff_cs, loc);
                    let sel = sel_cval_ext.as_ref().expect("wild_z non-zero requires constant sel or pat");
                    let eff_sel = CVal::four(cw, eff_cs, sel.bits_v().or(&wild_z), wild_z);
                    let hit = self.builder.alloc_op(NetOp::WildEq { rhs: eff_sel }, vec![p_ext], 1, false, pf, loc);
                    return Some((hit, None));
                }
            } else {
                let pat_v = self.lower_in_proc(&arena, pat_e, ctx)?.as_packed()?;
                let s_ext = self.builder.resize_val(sel_val, cw, eff_cs, eff_cs, loc);
                let p_ext = self.builder.resize_val(pat_v, cw, eff_cs, eff_cs, loc);
                let hit = if !wild_z.is_zero() {
                    let keep_c = self.builder.alloc_const(CVal::four(cw, eff_cs, keep_mask, Int::from(0)), false, loc);
                    let lhs_m = self.builder.alloc_op(NetOp::And, vec![s_ext, keep_c], cw, eff_cs, sf, loc);
                    let rhs_m = self.builder.alloc_op(NetOp::And, vec![p_ext, keep_c], cw, eff_cs, pf, loc);
                    self.builder.alloc_op(NetOp::CaseEq, vec![lhs_m, rhs_m], 1, false, false, loc)
                } else {
                    self.builder.alloc_op(NetOp::CaseEq, vec![s_ext, p_ext], 1, false, false, loc)
                };
                let cp = if has_nonwild_x {
                    None
                } else {
                    pat_cval.map(|val| {
                        if wild_z.is_zero() {
                            ConstCasePat::Exact { val, cw, cs: eff_cs }
                        } else {
                            ConstCasePat::Wild { val, wild_mask: wild_z, cw, cs: eff_cs }
                        }
                    })
                };
                return Some((hit, cp));
            }
        }

        // `casex`: both `x` and `z`/`?` (`cval.u`) are wildcards, symmetric across constant `sel` and `pat`.
        if is_casex {
            let pat_w = pat_cval.as_ref().map(|p| p.u.clone()).unwrap_or_else(|| Int::from(0));
            let sel_w = sel_cval_ext.as_ref().map(|s| s.u.clone()).unwrap_or_else(|| Int::from(0));
            let wild = pat_w.or(&sel_w).trunc(cw.max(1), false);
            if !self.config.allow_four_state_ops {
                if !wild.is_zero() {
                    if let Some(pat) = &pat_cval {
                        let s_ext = self.builder.resize_val(sel_val, cw, eff_cs, eff_cs, loc);
                        let eff_pat = CVal::four(cw, eff_cs, pat.bits_v().or(&wild), wild.clone());
                        let hit = self.builder.alloc_op(NetOp::WildEq { rhs: eff_pat }, vec![s_ext], 1, false, sf, loc);
                        let cp = ConstCasePat::Wild { val: pat.clone(), wild_mask: wild, cw, cs: eff_cs };
                        return Some((hit, Some(cp)));
                    }
                    let pat_v = self.lower_in_proc(&arena, pat_e, ctx)?.as_packed()?;
                    let p_ext = self.builder.resize_val(pat_v, cw, eff_cs, eff_cs, loc);
                    let sel = sel_cval_ext.as_ref().expect("wild non-zero requires constant sel or pat");
                    let eff_sel = CVal::four(cw, eff_cs, sel.bits_v().or(&wild), wild);
                    let hit = self.builder.alloc_op(NetOp::WildEq { rhs: eff_sel }, vec![p_ext], 1, false, pf, loc);
                    return Some((hit, None));
                }
            } else {
                let pat_v = self.lower_in_proc(&arena, pat_e, ctx)?.as_packed()?;
                let s_ext = self.builder.resize_val(sel_val, cw, eff_cs, eff_cs, loc);
                let p_ext = self.builder.resize_val(pat_v, cw, eff_cs, eff_cs, loc);
                let hit = if !wild.is_zero() {
                    let keep_mask = wild.not().trunc(cw.max(1), false);
                    let keep_c = self.builder.alloc_const(CVal::four(cw, eff_cs, keep_mask, Int::from(0)), false, loc);
                    let lhs_m = self.builder.alloc_op(NetOp::And, vec![s_ext, keep_c], cw, eff_cs, sf, loc);
                    let rhs_m = self.builder.alloc_op(NetOp::And, vec![p_ext, keep_c], cw, eff_cs, pf, loc);
                    self.builder.alloc_op(NetOp::CaseEq, vec![lhs_m, rhs_m], 1, false, false, loc)
                } else {
                    self.builder.alloc_op(NetOp::CaseEq, vec![s_ext, p_ext], 1, false, false, loc)
                };
                let cp = pat_cval.map(|val| {
                    if wild.is_zero() {
                        ConstCasePat::Exact { val, cw, cs: eff_cs }
                    } else {
                        ConstCasePat::Wild { val, wild_mask: wild, cw, cs: eff_cs }
                    }
                });
                return Some((hit, cp));
            }
        }

        // Plain `case` (or `casez`/`casex` in 2-state mode with zero wildcards/unknowns).
        if !self.config.allow_four_state_ops {
            let pat_has_unk = pat_cval.as_ref().is_some_and(|p| !p.known());
            let sel_has_unk = sel_cval_ext.as_ref().is_some_and(|s| !s.known());
            if pat_has_unk || sel_has_unk {
                let eq = match (&sel_cval_ext, &pat_cval) {
                    (Some(sc), Some(pc)) => sc == pc,
                    _ => false,
                };
                let hit = self.builder.alloc_const(CVal::new(1, false, i128::from(eq)), false, loc);
                return Some((hit, None));
            }
        }

        let pat_v = self.lower_in_proc(&arena, pat_e, ctx)?.as_packed()?;
        let s_ext = self.builder.resize_val(sel_val, cw, eff_cs, eff_cs, loc);
        let p_ext = self.builder.resize_val(pat_v, cw, eff_cs, eff_cs, loc);
        let cmp_op = if self.config.allow_four_state_ops { NetOp::CaseEq } else { NetOp::Eq };
        let out_four = if self.config.allow_four_state_ops { false } else { sf || pf };
        let hit = self.builder.alloc_op(cmp_op, vec![s_ext, p_ext], 1, false, out_four, loc);
        let cp = pat_cval.filter(|c| c.known()).map(|val| ConstCasePat::Exact { val, cw, cs: eff_cs });
        Some((hit, cp))
    }

    pub(crate) fn merge_branch_ctxs(
        &mut self,
        cond: ValueId,
        then_ctx: ProcCtx,
        else_ctx: ProcCtx,
        has_explicit_else: bool,
        loc: Loc,
    ) -> ProcCtx {
        let mut merged = else_ctx.clone();
        merged.returned_cond = self.builder.alloc_op(
            NetOp::Mux,
            vec![cond, then_ctx.returned_cond, else_ctx.returned_cond],
            1,
            false,
            false,
            loc,
        );
        merged.break_cond = self.builder.alloc_op(
            NetOp::Mux,
            vec![cond, then_ctx.break_cond, else_ctx.break_cond],
            1,
            false,
            false,
            loc,
        );
        merged.continue_cond = self.builder.alloc_op(
            NetOp::Mux,
            vec![cond, then_ctx.continue_cond, else_ctx.continue_cond],
            1,
            false,
            false,
            loc,
        );

        // Merge local variables.
        let mut all_lids: Vec<LocalId> = then_ctx.cur_locals.keys().copied().collect();
        for &lid in else_ctx.cur_locals.keys() {
            if !all_lids.contains(&lid) {
                all_lids.push(lid);
            }
        }
        for lid in all_lids {
            let tv = then_ctx.cur_locals.get(&lid).cloned();
            let ev = else_ctx.cur_locals.get(&lid).cloned();
            if let (Some(t_ov), Some(e_ov)) = (tv, ev) {
                let ty = merged
                    .local_decls
                    .values()
                    .find(|lb| lb.id == lid)
                    .map(|lb| lb.ty.clone())
                    .unwrap_or(RtlType::Void);
                let mv = mux_obj_val(&mut self.builder, cond, t_ov, e_ov, &ty, loc);
                let new_cval = mv.as_packed().and_then(|v| self.builder.const_cval(v).cloned());
                for lb in merged.local_decls.values_mut() {
                    if lb.id == lid {
                        lb.cval = new_cval.clone();
                    }
                }
                merged.cur_locals.insert(lid, mv);
            }
        }

        // Merge module symbol blocking-read maps (`cur_syms`).
        let mut all_sids: Vec<SymbolId> = then_ctx.cur_syms.keys().copied().collect();
        for &sid in else_ctx.cur_syms.keys() {
            if !all_sids.contains(&sid) {
                all_sids.push(sid);
            }
        }
        for sid in all_sids {
            let ty = self.design.symbol(sid).ty.clone();
            let slot = match self.sym_slots.get(&sid).cloned() {
                Some(s) => s,
                None => continue,
            };
            let t_ov = then_ctx.cur_syms.get(&sid).cloned().unwrap_or_else(|| self.read_slot_obj(&slot, loc));
            let e_ov = else_ctx.cur_syms.get(&sid).cloned().unwrap_or_else(|| self.read_slot_obj(&slot, loc));
            let mv = mux_obj_val(&mut self.builder, cond, t_ov, e_ov, &ty, loc);
            merged.cur_syms.insert(sid, mv);
        }

        // Merge per-slot writes (`slot_writes`).
        let mut all_slots: Vec<usize> = then_ctx.slot_writes.keys().copied().collect();
        for &sidx in else_ctx.slot_writes.keys() {
            if !all_slots.contains(&sidx) {
                all_slots.push(sidx);
            }
        }
        for sidx in all_slots {
            let w = self.slots[sidx].width;
            let s = self.slots[sidx].signed;
            let four = self.slots[sidx].four_state;
            let def_v = self.slot_placeholders[sidx];
            let zero1 = self.builder.alloc_const(CVal::new(1, false, 0), false, loc);

            let t_sw = then_ctx.slot_writes.get(&sidx);
            let e_sw = else_ctx.slot_writes.get(&sidx);
            let first_loc = t_sw.or(e_sw).map_or(loc, |x| x.first_loc);
            let has_blocking = t_sw.is_some_and(|x| x.has_blocking) || e_sw.is_some_and(|x| x.has_blocking);
            let has_nba = t_sw.is_some_and(|x| x.has_nba) || e_sw.is_some_and(|x| x.has_nba);

            let t_cur = t_sw.map_or(def_v, |x| x.cur_val);
            let e_cur = e_sw.map_or(def_v, |x| x.cur_val);
            let m_cur = self.builder.alloc_op(NetOp::Mux, vec![cond, t_cur, e_cur], w, s, four, loc);

            let t_nxt = t_sw.map_or(def_v, |x| x.next_val);
            let e_nxt = e_sw.map_or(def_v, |x| x.next_val);
            let m_nxt = self.builder.alloc_op(NetOp::Mux, vec![cond, t_nxt, e_nxt], w, s, four, loc);

            let mut ever = vec![false; w as usize];
            let mut def = vec![false; w as usize];
            let mut wcond = vec![zero1; w as usize];
            for b in 0..(w as usize) {
                let te = t_sw.is_some_and(|x| x.ever_written[b]);
                let ee = e_sw.is_some_and(|x| x.ever_written[b]);
                ever[b] = te || ee;

                let td = t_sw.is_some_and(|x| x.def_written[b]);
                let ed = e_sw.is_some_and(|x| x.def_written[b]);
                let _ = has_explicit_else;
                def[b] = td && ed;

                let twc = t_sw.map_or(zero1, |x| x.write_cond[b]);
                let ewc = e_sw.map_or(zero1, |x| x.write_cond[b]);
                wcond[b] = self.builder.alloc_op(NetOp::Mux, vec![cond, twc, ewc], 1, false, false, loc);
            }
            merged.slot_writes.insert(
                sidx,
                SlotWriteState {
                    cur_val: m_cur,
                    next_val: m_nxt,
                    ever_written: ever,
                    def_written: def,
                    write_cond: wcond,
                    first_loc,
                    has_blocking,
                    has_nba,
                },
            );
        }

        merged
    }

    fn exec_immediate_assert(&mut self, tree_idx: usize, stmt: NodeId, ctx: &mut ProcCtx) {
        let t = &self.trees[tree_idx];
        let loc = self.stmt_loc(tree_idx, stmt, ctx);
        let label = t
            .child(stmt, SyntaxKind::StmtLabel)
            .map(|l| t.bare_text(l).trim_end_matches(':').trim().to_string())
            .filter(|s| !s.is_empty());
        let own = own_toks(t, stmt);
        let kind = if own.contains(&"assume") {
            CheckKind::Assume
        } else if own.contains(&"cover") {
            CheckKind::Cover
        } else {
            CheckKind::Assert
        };
        let Some(cond_node) =
            t.children(stmt).find(|&c| !matches!(t.kind(c), SyntaxKind::Attribute | SyntaxKind::StmtLabel))
        else {
            return;
        };
        let Some(expr_val) = self.eval_proc_cond(tree_idx, cond_node, ctx) else { return };
        let enable = ctx.effective_reach(&mut self.builder, loc);
        self.builder.checks.push(Check {
            kind,
            label,
            immediate: true,
            enable,
            expr: expr_val,
            clock: ctx.active_clock,
            loc,
        });
    }

    fn exec_return_stmt(&mut self, tree_idx: usize, stmt: NodeId, ctx: &mut ProcCtx) {
        let t = &self.trees[tree_idx];
        let loc = self.stmt_loc(tree_idx, stmt, ctx);
        let expr_opt = t.children(stmt).find(|&c| t.kind(c) != SyntaxKind::Attribute);
        if let (Some(ret_expr), Some((ret_lid, ret_ty))) = (expr_opt, ctx.ret_local.clone()) {
            let mut arena = ExprArena::new();
            let mut binder = self.make_proc_binder(ctx, &mut arena);
            let eid_opt = binder.bind_assigned(tree_idx, ret_expr, &ret_ty);
            self.flush_expr_diags(binder.diags);
            if let Some(eid) = eid_opt {
                if let Some(val) = self.lower_in_proc(&arena, eid, ctx) {
                    let reach = ctx.local_reach(&mut self.builder, loc);
                    let prev = ctx
                        .cur_locals
                        .get(&ret_lid)
                        .cloned()
                        .unwrap_or_else(|| make_oob_obj_val(&mut self.builder, &ret_ty, loc));
                    let updated = mux_obj_val(&mut self.builder, reach, val, prev, &ret_ty, loc);
                    ctx.cur_locals.insert(ret_lid, updated);
                }
            }
        }
        let reach = ctx.local_reach(&mut self.builder, loc);
        ctx.returned_cond = self.builder.alloc_or1(ctx.returned_cond, reach, loc);
    }

    pub(crate) fn exec_expr_stmt(&mut self, tree_idx: usize, expr_node: NodeId, ctx: &mut ProcCtx) {
        let t = &self.trees[tree_idx];
        let loc = self.stmt_loc(tree_idx, expr_node, ctx);
        if ctx.in_initial {
            if t.kind(expr_node) == SyntaxKind::BinaryExpr {
                let kids: Vec<NodeId> = t.children(expr_node).filter(|&c| t.kind(c) != SyntaxKind::Attribute).collect();
                let op = own_toks(t, expr_node).into_iter().next().unwrap_or("");
                if kids.len() == 2 && matches!(op, "=" | "<=") {
                    self.exec_assignment(tree_idx, kids[0], op, Some(kids[1]), ctx, loc);
                    return;
                }
            }
            self.builder.unsup(loc, "unsupported expression statement or task call inside initial block");
            return;
        }
        match t.kind(expr_node) {
            SyntaxKind::BinaryExpr => {
                let kids: Vec<NodeId> = t.children(expr_node).filter(|&c| t.kind(c) != SyntaxKind::Attribute).collect();
                let op = own_toks(t, expr_node).into_iter().next().unwrap_or("");
                if kids.len() == 2 && is_assign_op(op) {
                    self.exec_assignment(tree_idx, kids[0], op, Some(kids[1]), ctx, loc);
                    return;
                }
            }
            SyntaxKind::PrefixExpr | SyntaxKind::PostfixExpr => {
                let op = own_toks(t, expr_node).into_iter().next().unwrap_or("");
                if matches!(op, "++" | "--") {
                    if let Some(target) = t.children(expr_node).find(|&c| t.kind(c) != SyntaxKind::Attribute) {
                        let assign_op = if op == "++" { "+=" } else { "-=" };
                        self.exec_assignment(tree_idx, target, assign_op, None, ctx, loc);
                        return;
                    }
                }
            }
            SyntaxKind::CallExpr => {
                self.exec_call_stmt(tree_idx, expr_node, ctx, loc);
                return;
            }
            SyntaxKind::NameExpr | SyntaxKind::ScopedExpr | SyntaxKind::MemberExpr => {
                // Zero-arg task call (`my_task;`, `pkg::my_task;`, or `bus.clear;`).
                self.exec_call_stmt(tree_idx, expr_node, ctx, loc);
                return;
            }
            _ => {}
        }
        // Other standalone expression (e.g. `void'(fn_call(...))`): bind and evaluate for side-effects/inlining.
        let mut arena = ExprArena::new();
        if let Some(eid) = self.bind_in_proc(tree_idx, expr_node, ctx, &mut arena, false) {
            let _ = self.lower_in_proc(&arena, eid, ctx);
        }
    }

    pub(crate) fn exec_assignment(
        &mut self,
        tree_idx: usize,
        lhs_node: NodeId,
        op: &str,
        rhs_node: Option<NodeId>,
        ctx: &mut ProcCtx,
        loc: Loc,
    ) {
        let is_nba = op == "<=";
        if is_nba && !matches!(ctx.kind, ProcKind::Clocked | ProcKind::AlwaysLatch | ProcKind::Subroutine) {
            self.builder.err(loc, "non-blocking assignment ('<=') is illegal inside combinational block");
            return;
        }
        self.check_modport_lhs_legality(ctx.inst_id, tree_idx, lhs_node, &ctx.scope_prefix, loc);

        let mut arena = ExprArena::new();
        let Some(lhs_eid) = self.bind_in_proc(tree_idx, lhs_node, ctx, &mut arena, false) else { return };
        let lhs_ty = arena.get(lhs_eid).ty.clone();

        // Concatenation LHS `{a, b} = rhs;` / `{a, b} <= rhs;`.
        if let ExprKind::Concat(parts) = arena.get(lhs_eid).kind.clone() {
            if op != "=" && op != "<=" {
                self.builder.err(loc, "compound assignment on concatenation LHS is illegal");
                return;
            }
            let Some(rn) = rhs_node else { return };
            let mut binder = self.make_proc_binder(ctx, &mut arena);
            let rhs_eid_opt = binder.bind_assigned(tree_idx, rn, &lhs_ty);
            self.flush_expr_diags(binder.diags);
            let Some(rhs_eid) = rhs_eid_opt else { return };
            let Some(rv) = self.lower_in_proc(&arena, rhs_eid, ctx).and_then(|o| o.as_packed()) else { return };
            if ctx.in_initial && self.builder.const_cval(rv).is_none() {
                self.builder.unsup(loc, "non-constant initial assignment is not supported");
                return;
            }
            let (total_w, _, four) = self.builder.node_shape(rv);
            let mut hi_cursor = total_w;
            for part_eid in parts {
                let pw = arena.get(part_eid).ty.bit_width().unwrap_or(1);
                if hi_cursor < pw {
                    break;
                }
                let lo = hi_cursor - pw;
                let slice =
                    self.builder.alloc_op(NetOp::Slice { hi: hi_cursor - 1, lo }, vec![rv], pw, false, four, loc);
                self.apply_bound_lhs_write(&arena, part_eid, ObjVal::Packed(slice), is_nba, ctx, loc);
                hi_cursor = lo;
            }
            return;
        }

        // Decompose LHS once so dynamic indices in `a[i] += b` or `a[i]++` are evaluated strictly once.
        let Some((root, steps)) = self.decompose_lhs(&arena, lhs_eid, ctx, loc) else { return };

        let final_rhs_val = if op == "=" || op == "<=" {
            let Some(rn) = rhs_node else { return };
            let mut binder = self.make_proc_binder(ctx, &mut arena);
            let rhs_eid_opt = binder.bind_assigned(tree_idx, rn, &lhs_ty);
            self.flush_expr_diags(binder.diags);
            let Some(rhs_eid) = rhs_eid_opt else { return };
            if self.reject_tristate_z(&arena, rhs_eid) {
                return;
            }
            let Some(val) = self.lower_in_proc(&arena, rhs_eid, ctx) else { return };
            if ctx.in_initial && !self.is_const_obj_val(&val) {
                self.builder.unsup(loc, "non-constant initial assignment is not supported");
                return;
            }
            val
        } else {
            // Compound assignment (`+=`, `-=`, ..., `++`, `--`): read current LHS value using pre-evaluated `steps`.
            let Some(cur_lhs_packed) = self.read_decomposed_lhs_packed(&root, &steps, &lhs_ty, ctx, loc) else {
                return;
            };
            let Some((lw, ls, l4)) = lhs_ty.integral_shape() else {
                self.builder.err(loc, "compound assignment requires integral/packed operand");
                return;
            };
            let is_shift = matches!(op, "<<=" | ">>=" | "<<<=" | ">>>=");
            let (rhs_packed, rw, rs, r4) = match rhs_node {
                Some(rn) => {
                    let re = if is_shift {
                        self.bind_in_proc(tree_idx, rn, ctx, &mut arena, false)
                    } else {
                        let (rhs_self_w, rhs_self_s) = {
                            let mut peek_arena = ExprArena::new();
                            let mut pb = self.make_proc_binder(ctx, &mut peek_arena);
                            pb.peek_self_shape(tree_idx, rn).map_or((lw, ls), |(w, s, _)| (w, s))
                        };
                        let cw = lw.max(rhs_self_w);
                        let cs = ls && rhs_self_s;
                        let mut binder = self.make_proc_binder(ctx, &mut arena);
                        let r = binder.bind_with_context(tree_idx, rn, cw, cs);
                        self.flush_expr_diags(binder.diags);
                        r
                    };
                    let Some(re) = re else { return };
                    let Some((rw, rs, r4)) = arena.get(re).ty.integral_shape() else {
                        self.builder.err(loc, "compound assignment right-hand side must be integral/packed");
                        return;
                    };
                    let Some(rv) = self.lower_in_proc(&arena, re, ctx).and_then(|o| o.as_packed()) else { return };
                    (rv, rw, rs, r4)
                }
                None => (self.builder.alloc_const(CVal::new(lw, ls, 1), false, loc), lw, ls, false),
            };
            let res = if is_shift {
                let nop = match op {
                    "<<=" | "<<<=" => NetOp::Shl,
                    ">>>=" if ls => NetOp::AShr,
                    _ => NetOp::LShr,
                };
                self.builder.alloc_op(nop, vec![cur_lhs_packed, rhs_packed], lw, ls, l4 || r4, loc)
            } else {
                let cw = lw.max(rw);
                let cs = ls && rs;
                let f4 = l4 || r4;
                let l_ext = self.builder.resize_val(cur_lhs_packed, cw, cs, cs, loc);
                let r_ext = self.builder.resize_val(rhs_packed, cw, cs, cs, loc);
                let nop = match op {
                    "+=" => NetOp::Add,
                    "-=" => NetOp::Sub,
                    "*=" => NetOp::Mul,
                    "/=" => {
                        if cs {
                            NetOp::SDiv
                        } else {
                            NetOp::UDiv
                        }
                    }
                    "%=" => {
                        if cs {
                            NetOp::SMod
                        } else {
                            NetOp::UMod
                        }
                    }
                    "&=" => NetOp::And,
                    "|=" => NetOp::Or,
                    _ => NetOp::Xor,
                };
                let wide = self.builder.alloc_op(nop, vec![l_ext, r_ext], cw, cs, f4, loc);
                self.builder.resize_val(wide, lw, cs, ls, loc)
            };
            ObjVal::Packed(res)
        };

        self.write_decomposed_lhs(root, &steps, final_rhs_val, is_nba, ctx, loc);
    }

    fn is_const_obj_val(&self, val: &ObjVal) -> bool {
        match val {
            ObjVal::Packed(v) => self.builder.const_cval(*v).is_some(),
            ObjVal::Struct(fields) => fields.iter().all(|(_, fv)| self.is_const_obj_val(fv)),
            ObjVal::Elems { elems, .. } => elems.iter().all(|ev| self.is_const_obj_val(ev)),
            ObjVal::Memory(_) => false,
        }
    }

    pub(crate) fn apply_bound_lhs_write(
        &mut self,
        arena: &ExprArena,
        lhs_eid: ExprId,
        rhs_val: ObjVal,
        is_nba: bool,
        ctx: &mut ProcCtx,
        loc: Loc,
    ) {
        if let Some((root, steps)) = self.decompose_lhs(arena, lhs_eid, ctx, loc) {
            self.write_decomposed_lhs(root, &steps, rhs_val, is_nba, ctx, loc);
        }
    }

    fn decompose_const_part_select_step(
        &mut self,
        bounds: (i128, i128),
        elem_width: u32,
        base_w: u32,
        low_idx: i128,
        count: usize,
        loc: Loc,
    ) -> LhsStep {
        let slice_total_w = (count as u32) * elem_width;
        let step = if bounds.0 >= bounds.1 { 1i128 } else { -1i128 };
        let mut first_in_bounds: Option<(usize, u32)> = None;
        let mut in_bounds_count = 0u32;
        for k in 0..count {
            let cur_idx = low_idx + (k as i128) * step;
            if let Some(off) = sv_semantic::expr::map_bit_offset(cur_idx, bounds, elem_width, base_w) {
                if first_in_bounds.is_none() {
                    first_in_bounds = Some((k, off));
                }
                in_bounds_count += 1;
            }
        }
        if let Some((k0, base_lo)) = first_in_bounds {
            let overlap_w = in_bounds_count * elem_width;
            let rhs_lo = (k0 as u32) * elem_width;
            if base_lo + overlap_w <= base_w {
                return LhsStep::PackedConstSlice { lo: base_lo, width: overlap_w, slice_total_w, rhs_lo };
            }
        }
        let zero = self.builder.alloc_const(CVal::new(1, false, 0), false, loc);
        LhsStep::PackedDynSlice { in_bounds: zero, bit_lo: zero, slice_w: slice_total_w }
    }

    fn decompose_lhs(
        &mut self,
        arena: &ExprArena,
        eid: ExprId,
        ctx: &mut ProcCtx,
        loc: Loc,
    ) -> Option<(LhsRoot, Vec<LhsStep>)> {
        let mut steps = Vec::new();
        let mut cur = eid;
        loop {
            let e = arena.get(cur);
            match &e.kind {
                ExprKind::Symbol(sid) => {
                    steps.reverse();
                    return Some((LhsRoot::Symbol(*sid), steps));
                }
                ExprKind::Local(lid) => {
                    steps.reverse();
                    return Some((LhsRoot::Local(*lid, e.ty.clone()), steps));
                }
                ExprKind::Member { base, field, packed_slice } => {
                    if let Some((lo, w)) = *packed_slice {
                        steps.push(LhsStep::PackedConstSlice { lo, width: w, slice_total_w: w, rhs_lo: 0 });
                    } else {
                        steps.push(LhsStep::StructField(field.clone()));
                    }
                    cur = *base;
                }
                ExprKind::BitSelect { base, index, bounds, elem_width } => {
                    let base_w = arena.get(*base).ty.bit_width().unwrap_or(1);
                    if let Some(icv) = arena.get(*index).cval.clone() {
                        if icv.known() {
                            if let Some(off) = sv_semantic::expr::map_bit_offset(icv.i(), *bounds, *elem_width, base_w)
                            {
                                steps.push(LhsStep::PackedConstSlice {
                                    lo: off,
                                    width: *elem_width,
                                    slice_total_w: *elem_width,
                                    rhs_lo: 0,
                                });
                                cur = *base;
                                continue;
                            }
                        }
                        let zero = self.builder.alloc_const(CVal::new(1, false, 0), false, loc);
                        steps.push(LhsStep::PackedDynSlice { in_bounds: zero, bit_lo: zero, slice_w: *elem_width });
                        cur = *base;
                        continue;
                    }
                    if ctx.in_initial {
                        self.builder.unsup(loc, "non-constant bit-select index on LHS inside initial block");
                        return None;
                    }
                    let iv = self.lower_in_proc(arena, *index, ctx)?.as_packed()?;
                    let idx_signed = arena.get(*index).ty.is_signed();
                    let (in_bounds, bit_lo) = proc_packed_offset(
                        &mut self.builder,
                        iv,
                        idx_signed,
                        *bounds,
                        *elem_width,
                        1,
                        true,
                        base_w,
                        loc,
                    );
                    steps.push(LhsStep::PackedDynSlice { in_bounds, bit_lo, slice_w: *elem_width });
                    cur = *base;
                }
                ExprKind::PartSelect { base, msb, lsb, bounds, elem_width } => {
                    let base_w = arena.get(*base).ty.bit_width().unwrap_or(1);
                    let count = ((*msb - *lsb).unsigned_abs() as usize) + 1;
                    let low_idx = if bounds.0 >= bounds.1 { (*msb).min(*lsb) } else { (*msb).max(*lsb) };
                    let step = self.decompose_const_part_select_step(*bounds, *elem_width, base_w, low_idx, count, loc);
                    steps.push(step);
                    cur = *base;
                }
                ExprKind::IndexedPartSelect { base, base_idx, width, ascending, bounds, elem_width } => {
                    let base_w = arena.get(*base).ty.bit_width().unwrap_or(1);
                    let slice_w = (*width) * (*elem_width);
                    if let Some(icv) = arena.get(*base_idx).cval.clone() {
                        if icv.known() {
                            let cnt = *width as usize;
                            let lo_sv = if *ascending { icv.i() } else { icv.i() - ((*width as i128) - 1) };
                            let hi_sv = lo_sv + ((*width as i128) - 1);
                            let low_idx = if bounds.0 >= bounds.1 { lo_sv } else { hi_sv };
                            let step =
                                self.decompose_const_part_select_step(*bounds, *elem_width, base_w, low_idx, cnt, loc);
                            steps.push(step);
                            cur = *base;
                            continue;
                        }
                        let zero = self.builder.alloc_const(CVal::new(1, false, 0), false, loc);
                        steps.push(LhsStep::PackedDynSlice { in_bounds: zero, bit_lo: zero, slice_w });
                        cur = *base;
                        continue;
                    }
                    if ctx.in_initial {
                        self.builder.unsup(loc, "non-constant indexed part-select on LHS inside initial block");
                        return None;
                    }
                    let iv = self.lower_in_proc(arena, *base_idx, ctx)?.as_packed()?;
                    let idx_signed = arena.get(*base_idx).ty.is_signed();
                    if *width <= 1 {
                        let (in_bounds, bit_lo) = proc_packed_offset(
                            &mut self.builder,
                            iv,
                            idx_signed,
                            *bounds,
                            *elem_width,
                            1,
                            *ascending,
                            base_w,
                            loc,
                        );
                        steps.push(LhsStep::PackedDynSlice { in_bounds, bit_lo, slice_w });
                    } else {
                        steps.push(LhsStep::PackedIndexedSlice {
                            idx_val: iv,
                            idx_signed,
                            bounds: *bounds,
                            elem_w: *elem_width,
                            count: *width,
                            ascending: *ascending,
                        });
                    }
                    cur = *base;
                }
                ExprKind::ArrayIndex { base, index, dim } => {
                    let iv = self.lower_in_proc(arena, *index, ctx)?.as_packed()?;
                    if ctx.in_initial && self.builder.const_cval(iv).is_none() {
                        self.builder.unsup(loc, "non-constant array index on LHS inside initial block");
                        return None;
                    }
                    let idx_signed = arena.get(*index).ty.is_signed();
                    steps.push(LhsStep::ArrayElem { idx_val: iv, idx_signed, dim: *dim, elem_ty: e.ty.clone() });
                    cur = *base;
                }
                _ => {
                    self.builder.err(loc, "invalid assignment left-hand side expression");
                    return None;
                }
            }
        }
    }

    fn read_decomposed_lhs_packed(
        &mut self,
        root: &LhsRoot,
        steps: &[LhsStep],
        lhs_ty: &RtlType,
        ctx: &ProcCtx,
        loc: Loc,
    ) -> Option<ValueId> {
        let mut cur = match root {
            LhsRoot::Symbol(sid) => {
                if let Some(ov) = ctx.cur_syms.get(sid) {
                    ov.clone()
                } else {
                    let slot = self.sym_slots.get(sid)?.clone();
                    self.read_slot_obj(&slot, loc)
                }
            }
            LhsRoot::Local(lid, rty) => {
                ctx.cur_locals.get(lid).cloned().unwrap_or_else(|| make_oob_obj_val(&mut self.builder, rty, loc))
            }
        };

        for step in steps {
            match step {
                LhsStep::StructField(f) => {
                    if let ObjVal::Struct(fields) = cur {
                        cur = fields.into_iter().find(|(n, _)| n == f)?.1;
                    } else {
                        return None;
                    }
                }
                LhsStep::PackedConstSlice { lo, width, slice_total_w, rhs_lo } => {
                    let pv = cur.as_packed()?;
                    let (_, _, four) = self.builder.node_shape(pv);
                    let in_part = self.builder.alloc_op(
                        NetOp::Slice { hi: lo + width - 1, lo: *lo },
                        vec![pv],
                        *width,
                        false,
                        four,
                        loc,
                    );
                    if *rhs_lo == 0 && *width == *slice_total_w {
                        cur = ObjVal::Packed(in_part);
                    } else {
                        let mut parts = Vec::new();
                        if *rhs_lo + *width < *slice_total_w {
                            let hi_w = *slice_total_w - (*rhs_lo + *width);
                            let pad_cv = if four { CVal::all_x(hi_w, false) } else { CVal::new(hi_w, false, 0) };
                            parts.push(self.builder.alloc_const(pad_cv, four, loc));
                        }
                        parts.push(in_part);
                        if *rhs_lo > 0 {
                            let pad_cv = if four { CVal::all_x(*rhs_lo, false) } else { CVal::new(*rhs_lo, false, 0) };
                            parts.push(self.builder.alloc_const(pad_cv, four, loc));
                        }
                        cur = ObjVal::Packed(self.builder.alloc_op(
                            NetOp::Concat,
                            parts,
                            *slice_total_w,
                            false,
                            four,
                            loc,
                        ));
                    }
                }
                LhsStep::PackedDynSlice { in_bounds, bit_lo, slice_w } => {
                    let pv = cur.as_packed()?;
                    let (total_w, _, four) = self.builder.node_shape(pv);
                    let shifted = self.builder.alloc_op(NetOp::LShr, vec![pv, *bit_lo], total_w, false, four, loc);
                    let ext = self.builder.resize_val(shifted, *slice_w, false, false, loc);
                    let oob_cv = if four { CVal::all_x(*slice_w, false) } else { CVal::new(*slice_w, false, 0) };
                    let oob = self.builder.alloc_const(oob_cv, four, loc);
                    cur = ObjVal::Packed(self.builder.alloc_op(
                        NetOp::Mux,
                        vec![*in_bounds, ext, oob],
                        *slice_w,
                        false,
                        four,
                        loc,
                    ));
                }
                LhsStep::PackedIndexedSlice { idx_val, idx_signed, bounds, elem_w, count, ascending } => {
                    let pv = cur.as_packed()?;
                    let (_, _, four) = self.builder.node_shape(pv);
                    cur = ObjVal::Packed(lower_dynamic_packed_slice(
                        &mut self.builder,
                        pv,
                        *idx_val,
                        *idx_signed,
                        *bounds,
                        *elem_w,
                        *count,
                        *ascending,
                        false,
                        four,
                        loc,
                    ));
                }
                LhsStep::ArrayElem { idx_val, idx_signed, dim, elem_ty } => match cur {
                    ObjVal::Memory(mid) => {
                        let (w, s, four) = elem_ty.integral_shape()?;
                        let min_b = dim.0.min(dim.1);
                        let max_b = dim.0.max(dim.1);
                        let addr_w = proc_addr_bits(((max_b - min_b) as u32) + 1);
                        let (in_b, addr) =
                            proc_unpacked_bounds(&mut self.builder, *idx_val, *idx_signed, min_b, max_b, addr_w, loc);
                        let rd = self.builder.alloc_raw(w, s, four, BuildKind::MemRead { mem: mid, addr }, None, loc);
                        let oob_cv = if four { CVal::all_x(w, s) } else { CVal::new(w, s, 0) };
                        let oob = self.builder.alloc_const(oob_cv, four, loc);
                        cur = ObjVal::Packed(self.builder.alloc_op(NetOp::Mux, vec![in_b, rd, oob], w, s, four, loc));
                    }
                    ObjVal::Elems { left, right, elems } => {
                        let min_b = left.min(right);
                        let max_b = left.max(right);
                        let addr_w = proc_addr_bits(elems.len() as u32);
                        let (in_b, addr) =
                            proc_unpacked_bounds(&mut self.builder, *idx_val, *idx_signed, min_b, max_b, addr_w, loc);
                        let mut acc = make_oob_obj_val(&mut self.builder, elem_ty, loc);
                        for (k, el) in elems.into_iter().enumerate().rev() {
                            let kc = self.builder.alloc_const(CVal::new(addr_w, false, k as i128), false, loc);
                            let eq = self.builder.alloc_op(NetOp::Eq, vec![addr, kc], 1, false, false, loc);
                            let hit = self.builder.alloc_and1(in_b, eq, loc);
                            acc = mux_obj_val(&mut self.builder, hit, el, acc, elem_ty, loc);
                        }
                        cur = acc;
                    }
                    _ => return None,
                },
            }
        }
        let _ = lhs_ty;
        cur.as_packed()
    }

    fn write_decomposed_lhs(
        &mut self,
        root: LhsRoot,
        steps: &[LhsStep],
        rhs_val: ObjVal,
        is_nba: bool,
        ctx: &mut ProcCtx,
        loc: Loc,
    ) {
        match root {
            LhsRoot::Local(lid, root_ty) => {
                let loc_reach = if ctx.unconditional_lids.contains(&lid) {
                    self.builder.alloc_const(CVal::new(1, false, 1), false, loc)
                } else {
                    ctx.local_reach(&mut self.builder, loc)
                };
                let old_obj = ctx
                    .cur_locals
                    .get(&lid)
                    .cloned()
                    .unwrap_or_else(|| make_oob_obj_val(&mut self.builder, &root_ty, loc));
                let updated = self.apply_steps_to_obj_val(old_obj, steps, rhs_val, loc_reach, &root_ty, loc);
                let new_cval = updated.as_packed().and_then(|v| self.builder.const_cval(v).cloned());
                for lb in ctx.local_decls.values_mut() {
                    if lb.id == lid {
                        lb.cval = new_cval.clone();
                    }
                }
                ctx.cur_locals.insert(lid, updated);
            }
            LhsRoot::Symbol(sid) => {
                let reach = ctx.local_reach(&mut self.builder, loc);
                let sym = self.design.symbol(sid);
                if matches!(
                    sym.kind,
                    SymbolKind::Port { dir: PortDir::Input, .. } | SymbolKind::Param { .. } | SymbolKind::Genvar
                ) {
                    self.builder.err(loc, format!("cannot procedurally assign to input/parameter '{}'", sym.full_path));
                    return;
                }
                let sym_ty = sym.ty.clone();
                let Some(slot) = self.sym_slots.get(&sid).cloned() else { return };
                self.write_symbol_slot(&slot, steps, rhs_val, is_nba, reach, ctx, loc);
                if !is_nba {
                    // Refresh `cur_syms[sid]` from updated `slot_writes` so subsequent blocking reads see the write.
                    let refreshed = self.read_symbol_current_in_proc(&slot, ctx, loc);
                    let _ = sym_ty;
                    ctx.cur_syms.insert(sid, refreshed);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn write_symbol_slot(
        &mut self,
        slot: &ObjSlot,
        steps: &[LhsStep],
        rhs_val: ObjVal,
        is_nba: bool,
        reach: ValueId,
        ctx: &mut ProcCtx,
        loc: Loc,
    ) {
        match (slot, steps.first()) {
            (ObjSlot::Struct(fields), Some(LhsStep::StructField(fname))) => {
                if let Some((_, fslot)) = fields.iter().find(|(n, _)| n == fname) {
                    self.write_symbol_slot(fslot, &steps[1..], rhs_val, is_nba, reach, ctx, loc);
                }
            }
            (ObjSlot::Struct(fields), None) => {
                if let ObjVal::Struct(vfields) = rhs_val {
                    for (fname, fslot) in fields {
                        if let Some((_, fv)) = vfields.iter().find(|(n, _)| n == fname) {
                            self.write_symbol_slot(fslot, &[], fv.clone(), is_nba, reach, ctx, loc);
                        }
                    }
                }
            }
            (ObjSlot::Elems { left, right, elems }, Some(LhsStep::ArrayElem { idx_val, idx_signed, .. })) => {
                let min_b = (*left).min(*right);
                let max_b = (*left).max(*right);
                if let Some(icv) = self.builder.const_cval(*idx_val).cloned() {
                    if icv.known() && icv.i() >= min_b && icv.i() <= max_b {
                        let off = (icv.i() - min_b) as usize;
                        if let Some(eslot) = elems.get(off) {
                            self.write_symbol_slot(eslot, &steps[1..], rhs_val, is_nba, reach, ctx, loc);
                        }
                    }
                    // Static OOB write is a guaranteed no-op.
                    return;
                }
                let addr_w = proc_addr_bits(elems.len() as u32);
                let (in_b, addr) =
                    proc_unpacked_bounds(&mut self.builder, *idx_val, *idx_signed, min_b, max_b, addr_w, loc);
                for (k, eslot) in elems.iter().enumerate() {
                    let kc = self.builder.alloc_const(CVal::new(addr_w, false, k as i128), false, loc);
                    let eq = self.builder.alloc_op(NetOp::Eq, vec![addr, kc], 1, false, false, loc);
                    let hit = self.builder.alloc_and1(in_b, eq, loc);
                    let elem_reach = self.builder.alloc_and1(reach, hit, loc);
                    self.write_symbol_slot(eslot, &steps[1..], rhs_val.clone(), is_nba, elem_reach, ctx, loc);
                }
            }
            (ObjSlot::Elems { elems, .. }, None) => {
                if let ObjVal::Elems { elems: velems, .. } = rhs_val {
                    for (eslot, ev) in elems.iter().zip(velems) {
                        self.write_symbol_slot(eslot, &[], ev, is_nba, reach, ctx, loc);
                    }
                }
            }
            (ObjSlot::Memory(mid), Some(LhsStep::ArrayElem { idx_val, idx_signed, dim, elem_ty })) => {
                let min_b = dim.0.min(dim.1);
                let max_b = dim.0.max(dim.1);
                let depth = ((max_b - min_b) as u32) + 1;
                let addr_w = proc_addr_bits(depth);
                let (in_b, norm_addr) =
                    proc_unpacked_bounds(&mut self.builder, *idx_val, *idx_signed, min_b, max_b, addr_w, loc);
                // If constant OOB (`in_b == 0`), skip emitting a dead write port completely.
                if let Some(ib_cv) = self.builder.const_cval(in_b) {
                    if ib_cv.known() && !ib_cv.is_true() {
                        return;
                    }
                }
                let mem_reach = self.builder.alloc_and1(ctx.path_cond, reach, loc);
                let raw_write_en = self.builder.alloc_and1(mem_reach, in_b, loc);
                let write_en = coerce_two_state_en(&mut self.builder, raw_write_en, loc);
                let (mem_w, mem_s, mem_4) = {
                    let m = &self.builder.memories[mid.0 as usize];
                    (m.width, m.signed, m.four_state)
                };
                let word_data = if steps.len() == 1 {
                    let Some(pv) = rhs_val.as_packed() else { return };
                    self.builder.resize_val(pv, mem_w, false, mem_s, loc)
                } else {
                    // Partial bit/member write inside a memory word (`mem[addr][3:0] <= nibble`):
                    // chain over any earlier writes emitted in the same procedural block so
                    // back-to-back partial-word NBAs (`mem[a][3:0] <= x; mem[a][7:4] <= y;`) compose.
                    let mut old_word = self.builder.alloc_raw(
                        mem_w,
                        mem_s,
                        mem_4,
                        BuildKind::MemRead { mem: *mid, addr: norm_addr },
                        None,
                        loc,
                    );
                    let start = ctx
                        .mem_writes_start
                        .get(mid.0 as usize)
                        .copied()
                        .unwrap_or(self.builder.memories[mid.0 as usize].writes.len());
                    let prior_writes: Vec<(ValueId, ValueId, ValueId)> = self.builder.memories[mid.0 as usize].writes
                        [start..]
                        .iter()
                        .map(|wp| (wp.addr, wp.data, wp.enable))
                        .collect();
                    for (p_addr, p_data, p_en) in prior_writes {
                        let same_addr = self.builder.alloc_op(NetOp::Eq, vec![p_addr, norm_addr], 1, false, false, loc);
                        let same_addr_2s = coerce_two_state_en(&mut self.builder, same_addr, loc);
                        let hit = self.builder.alloc_and1(p_en, same_addr_2s, loc);
                        old_word =
                            self.builder.alloc_op(NetOp::Mux, vec![hit, p_data, old_word], mem_w, mem_s, mem_4, loc);
                    }
                    let one = self.builder.alloc_const(CVal::new(1, false, 1), false, loc);
                    let updated =
                        self.apply_steps_to_obj_val(ObjVal::Packed(old_word), &steps[1..], rhs_val, one, elem_ty, loc);
                    let Some(pv) = updated.as_packed() else { return };
                    self.builder.resize_val(pv, mem_w, false, mem_s, loc)
                };
                self.builder.memories[mid.0 as usize].writes.push(MemWritePort {
                    clock: ctx.active_clock,
                    addr: norm_addr,
                    data: word_data,
                    enable: write_en,
                    loc,
                });
            }
            (ObjSlot::Packed(sidx), _) => {
                let Some(rhs_packed) = rhs_val.as_packed() else { return };
                self.write_packed_slot(*sidx, steps, rhs_packed, is_nba, reach, ctx, loc);
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn write_packed_slot(
        &mut self,
        sidx: usize,
        steps: &[LhsStep],
        mut rhs_packed: ValueId,
        is_nba: bool,
        reach: ValueId,
        ctx: &mut ProcCtx,
        loc: Loc,
    ) {
        let total_w = self.slots[sidx].width;
        let s_signed = self.slots[sidx].signed;
        let s_four = self.slots[sidx].four_state;
        let placeholder = self.slot_placeholders[sidx];
        let zero1 = self.builder.alloc_const(CVal::new(1, false, 0), false, loc);
        let unconditional = ctx.jump_is_clean(&self.builder);

        // Expand multi-lane dynamic indexed part-select (`v[idx +: count] = rhs`) into per-lane writes
        // so partial out-of-bounds lanes are discarded while in-bounds lanes commit cleanly.
        if let Some(pi) = steps.iter().position(|st| matches!(st, LhsStep::PackedIndexedSlice { .. })) {
            let LhsStep::PackedIndexedSlice { idx_val, idx_signed, bounds, elem_w, count, ascending } =
                steps[pi].clone()
            else {
                unreachable!()
            };
            let prefix_w = steps[..pi]
                .iter()
                .rev()
                .find_map(|st| match st {
                    LhsStep::PackedConstSlice { width, .. } => Some(*width),
                    _ => None,
                })
                .unwrap_or(total_w);
            let slice_total_w = count * elem_w;
            let rhs_sized = self.builder.resize_val(rhs_packed, slice_total_w, false, false, loc);
            let (_, _, rf) = self.builder.node_shape(rhs_sized);
            let (iw, _, if4) = self.builder.node_shape(idx_val);
            let calc_w = iw.max(32).max(proc_addr_bits(prefix_w)) + 2;
            let ext_idx = self.builder.resize_val(idx_val, calc_w, idx_signed, true, loc);
            let lo_sv = if ascending {
                ext_idx
            } else {
                let span_c = self.builder.alloc_const(CVal::new(calc_w, true, (count - 1) as i128), false, loc);
                self.builder.alloc_op(NetOp::Sub, vec![ext_idx, span_c], calc_w, true, if4, loc)
            };
            for k in 0..count {
                let sv_off = if bounds.0 >= bounds.1 { k } else { count - 1 - k };
                let lane_sv = if sv_off == 0 {
                    lo_sv
                } else {
                    let kc = self.builder.alloc_const(CVal::new(calc_w, true, sv_off as i128), false, loc);
                    self.builder.alloc_op(NetOp::Add, vec![lo_sv, kc], calc_w, true, if4, loc)
                };
                let (in_b_k, bit_lo_k) =
                    proc_packed_offset(&mut self.builder, lane_sv, true, bounds, elem_w, 1, true, prefix_w, loc);
                let rhs_k = self.builder.alloc_op(
                    NetOp::Slice { hi: (k + 1) * elem_w - 1, lo: k * elem_w },
                    vec![rhs_sized],
                    elem_w,
                    false,
                    rf,
                    loc,
                );
                let mut lane_steps = Vec::with_capacity(steps.len());
                lane_steps.extend_from_slice(&steps[..pi]);
                lane_steps.push(LhsStep::PackedDynSlice { in_bounds: in_b_k, bit_lo: bit_lo_k, slice_w: elem_w });
                lane_steps.extend_from_slice(&steps[pi + 1..]);
                self.write_packed_slot(sidx, &lane_steps, rhs_k, is_nba, reach, ctx, loc);
            }
            return;
        }

        // Collapse constant slice steps, accounting for partial-OOB constant part-selects (`rhs_lo` / `slice_total_w`).
        let mut const_lo = 0u32;
        let mut const_w = total_w;
        let mut cur_rhs_lo = 0u32;
        let mut cur_slice_total_w = total_w;
        let mut dyn_step_idx: Option<usize> = None;
        for (i, st) in steps.iter().enumerate() {
            match st {
                LhsStep::PackedConstSlice { lo, width, slice_total_w, rhs_lo } => {
                    // Compose `[lo .. lo + width)` within parent's `[cur_rhs_lo .. cur_rhs_lo + const_w)`.
                    let eff_start = (*lo).max(cur_rhs_lo);
                    let eff_end = (*lo + *width).min(cur_rhs_lo + const_w);
                    if eff_start >= eff_end {
                        return;
                    }
                    const_lo += eff_start - cur_rhs_lo;
                    const_w = eff_end - eff_start;
                    cur_rhs_lo = *rhs_lo + (eff_start - *lo);
                    cur_slice_total_w = *slice_total_w;
                }
                LhsStep::PackedDynSlice { .. } => {
                    dyn_step_idx = Some(i);
                    break;
                }
                _ => {}
            }
        }

        if dyn_step_idx.is_none() && (cur_rhs_lo > 0 || const_w < cur_slice_total_w) {
            let rhs_full = self.builder.resize_val(rhs_packed, cur_slice_total_w, false, false, loc);
            let (_, _, rf) = self.builder.node_shape(rhs_full);
            rhs_packed = self.builder.alloc_op(
                NetOp::Slice { hi: cur_rhs_lo + const_w - 1, lo: cur_rhs_lo },
                vec![rhs_full],
                const_w,
                false,
                rf,
                loc,
            );
        }

        // Ensure `SlotWriteState` entry exists and check blocking/NBA mixing in clocked blocks.
        let (prev_has_blk, prev_has_nba) = {
            let sw = ctx.slot_writes.entry(sidx).or_insert_with(|| SlotWriteState {
                cur_val: placeholder,
                next_val: placeholder,
                ever_written: vec![false; total_w as usize],
                def_written: vec![false; total_w as usize],
                write_cond: vec![zero1; total_w as usize],
                first_loc: loc,
                has_blocking: false,
                has_nba: false,
            });
            let prev = (sw.has_blocking, sw.has_nba);
            if is_nba {
                sw.has_nba = true;
            } else {
                sw.has_blocking = true;
            }
            prev
        };
        if ctx.kind == ProcKind::Clocked
            && !ctx.in_initial
            && ((is_nba && prev_has_blk && !prev_has_nba) || (!is_nba && prev_has_nba && !prev_has_blk))
        {
            self.builder.err(
                loc,
                format!(
                    "mixing blocking ('=') and non-blocking ('<=') assignments to '{}' in one clocked block is illegal",
                    self.slots[sidx].name
                ),
            );
        }

        if let Some(di) = dyn_step_idx {
            let LhsStep::PackedDynSlice { in_bounds, bit_lo, slice_w } = steps[di].clone() else { unreachable!() };
            if let Some(ib_cv) = self.builder.const_cval(in_bounds) {
                if ib_cv.known() && !ib_cv.is_true() {
                    return;
                }
            }
            let write_en = self.builder.alloc_and1(reach, in_bounds, loc);
            let total_bit_lo = if const_lo == 0 {
                bit_lo
            } else {
                let (bw, _, bf) = self.builder.node_shape(bit_lo);
                let c_lo = self.builder.alloc_const(CVal::new(bw, false, const_lo as i128), false, loc);
                self.builder.alloc_op(NetOp::Add, vec![bit_lo, c_lo], bw, false, bf, loc)
            };

            let write_slice_val = if di + 1 < steps.len() {
                let base_src = if is_nba { ctx.slot_writes[&sidx].next_val } else { ctx.slot_writes[&sidx].cur_val };
                let shifted =
                    self.builder.alloc_op(NetOp::LShr, vec![base_src, total_bit_lo], total_w, false, s_four, loc);
                let old_sub = self.builder.resize_val(shifted, slice_w, false, false, loc);
                let sub_ty = RtlType::Integral { width: slice_w, signed: false, four_state: s_four };
                let one = self.builder.alloc_const(CVal::new(1, false, 1), false, loc);
                self.apply_steps_to_obj_val(
                    ObjVal::Packed(old_sub),
                    &steps[di + 1..],
                    ObjVal::Packed(rhs_packed),
                    one,
                    &sub_ty,
                    loc,
                )
                .as_packed()
                .unwrap_or(rhs_packed)
            } else {
                rhs_packed
            };

            let prev_nxt = ctx.slot_writes[&sidx].next_val;
            let new_nxt = rmw_dynamic_packed_slice(
                &mut self.builder,
                prev_nxt,
                write_slice_val,
                total_bit_lo,
                slice_w,
                write_en,
                loc,
            );
            let prev_cur = ctx.slot_writes[&sidx].cur_val;
            let new_cur = if is_nba {
                prev_cur
            } else {
                rmw_dynamic_packed_slice(
                    &mut self.builder,
                    prev_cur,
                    write_slice_val,
                    total_bit_lo,
                    slice_w,
                    write_en,
                    loc,
                )
            };

            let hi_bound = (const_lo + const_w).min(total_w) as usize;
            let (bw, _, _) = self.builder.node_shape(total_bit_lo);
            let mut new_wconds: Vec<(usize, ValueId)> = Vec::new();
            for b in (const_lo as usize)..hi_bound {
                let bc = self.builder.alloc_const(CVal::new(bw, false, b as i128), false, loc);
                let hit_b = if slice_w == 1 {
                    self.builder.alloc_op(NetOp::Eq, vec![total_bit_lo, bc], 1, false, false, loc)
                } else {
                    let ge_lo = self.builder.alloc_op(NetOp::ULe, vec![total_bit_lo, bc], 1, false, false, loc);
                    let sw_c = self.builder.alloc_const(CVal::new(bw, false, slice_w as i128), false, loc);
                    let end_lo = self.builder.alloc_op(NetOp::Add, vec![total_bit_lo, sw_c], bw, false, false, loc);
                    let lt_hi = self.builder.alloc_op(NetOp::ULt, vec![bc, end_lo], 1, false, false, loc);
                    self.builder.alloc_and1(ge_lo, lt_hi, loc)
                };
                let eff_b = self.builder.alloc_and1(write_en, hit_b, loc);
                let prev_wc = ctx.slot_writes[&sidx].write_cond[b];
                new_wconds.push((b, self.builder.alloc_or1(prev_wc, eff_b, loc)));
            }
            let sw = ctx.slot_writes.get_mut(&sidx).expect("inserted above");
            sw.next_val = new_nxt;
            sw.cur_val = new_cur;
            for (b, wc) in new_wconds {
                sw.ever_written[b] = true;
                sw.write_cond[b] = wc;
            }
            return;
        }

        // Constant slice `[const_lo .. const_lo + const_w - 1]`.
        let prev_nxt = ctx.slot_writes[&sidx].next_val;
        let replaced_nxt = replace_const_packed_slice(&mut self.builder, prev_nxt, rhs_packed, const_lo, const_w, loc);
        let new_nxt =
            self.builder.alloc_op(NetOp::Mux, vec![reach, replaced_nxt, prev_nxt], total_w, s_signed, s_four, loc);

        let prev_cur = ctx.slot_writes[&sidx].cur_val;
        let new_cur = if is_nba {
            prev_cur
        } else {
            let replaced_cur =
                replace_const_packed_slice(&mut self.builder, prev_cur, rhs_packed, const_lo, const_w, loc);
            self.builder.alloc_op(NetOp::Mux, vec![reach, replaced_cur, prev_cur], total_w, s_signed, s_four, loc)
        };

        let hi_bound = (const_lo + const_w).min(total_w) as usize;
        let mut new_wconds = Vec::with_capacity(hi_bound.saturating_sub(const_lo as usize));
        for b in (const_lo as usize)..hi_bound {
            let prev_wc = ctx.slot_writes[&sidx].write_cond[b];
            new_wconds.push((b, self.builder.alloc_or1(prev_wc, reach, loc)));
        }
        let sw = ctx.slot_writes.get_mut(&sidx).expect("inserted above");
        sw.next_val = new_nxt;
        sw.cur_val = new_cur;
        for (b, wc) in new_wconds {
            sw.ever_written[b] = true;
            if unconditional {
                sw.def_written[b] = true;
            }
            sw.write_cond[b] = wc;
        }
    }

    fn apply_steps_to_obj_val(
        &mut self,
        old_obj: ObjVal,
        steps: &[LhsStep],
        rhs_val: ObjVal,
        reach: ValueId,
        cur_ty: &RtlType,
        loc: Loc,
    ) -> ObjVal {
        let Some(first) = steps.first() else {
            return mux_obj_val(&mut self.builder, reach, rhs_val, old_obj, cur_ty, loc);
        };
        match (old_obj, first) {
            (ObjVal::Struct(mut fields), LhsStep::StructField(fname)) => {
                let fty = match cur_ty {
                    RtlType::UnpackedStruct { fields: tf, .. } => {
                        tf.iter().find(|(n, _)| n == fname).map(|(_, t)| t.clone()).unwrap_or(RtlType::Void)
                    }
                    _ => RtlType::Void,
                };
                for (n, fv) in &mut fields {
                    if n == fname {
                        let prev = fv.clone();
                        *fv = self.apply_steps_to_obj_val(prev, &steps[1..], rhs_val, reach, &fty, loc);
                        break;
                    }
                }
                ObjVal::Struct(fields)
            }
            (ObjVal::Elems { left, right, mut elems }, LhsStep::ArrayElem { idx_val, idx_signed, .. }) => {
                let sub_ty = match cur_ty {
                    RtlType::UnpackedArray { elem, dims } if dims.len() > 1 => {
                        RtlType::UnpackedArray { elem: elem.clone(), dims: dims[1..].to_vec() }
                    }
                    RtlType::UnpackedArray { elem, .. } => (**elem).clone(),
                    _ => RtlType::Void,
                };
                let min_b = left.min(right);
                let max_b = left.max(right);
                let addr_w = proc_addr_bits(elems.len() as u32);
                let (in_b, addr) =
                    proc_unpacked_bounds(&mut self.builder, *idx_val, *idx_signed, min_b, max_b, addr_w, loc);
                for (k, el) in elems.iter_mut().enumerate() {
                    let kc = self.builder.alloc_const(CVal::new(addr_w, false, k as i128), false, loc);
                    let eq = self.builder.alloc_op(NetOp::Eq, vec![addr, kc], 1, false, false, loc);
                    let hit = self.builder.alloc_and1(in_b, eq, loc);
                    let elem_reach = self.builder.alloc_and1(reach, hit, loc);
                    let prev = el.clone();
                    *el = self.apply_steps_to_obj_val(prev, &steps[1..], rhs_val.clone(), elem_reach, &sub_ty, loc);
                }
                ObjVal::Elems { left, right, elems }
            }
            (ObjVal::Packed(old_v), LhsStep::PackedConstSlice { lo, width, slice_total_w, rhs_lo }) => {
                let (tw, ts, tf) = self.builder.node_shape(old_v);
                let in_sub = self.builder.alloc_op(
                    NetOp::Slice { hi: lo + width - 1, lo: *lo },
                    vec![old_v],
                    *width,
                    false,
                    tf,
                    loc,
                );
                let old_sub = if *rhs_lo == 0 && *width == *slice_total_w {
                    in_sub
                } else {
                    let mut parts = Vec::new();
                    if *rhs_lo + *width < *slice_total_w {
                        let hi_w = *slice_total_w - (*rhs_lo + *width);
                        let pad = if tf { CVal::all_x(hi_w, false) } else { CVal::new(hi_w, false, 0) };
                        parts.push(self.builder.alloc_const(pad, tf, loc));
                    }
                    parts.push(in_sub);
                    if *rhs_lo > 0 {
                        let pad = if tf { CVal::all_x(*rhs_lo, false) } else { CVal::new(*rhs_lo, false, 0) };
                        parts.push(self.builder.alloc_const(pad, tf, loc));
                    }
                    self.builder.alloc_op(NetOp::Concat, parts, *slice_total_w, false, tf, loc)
                };
                let sub_ty = RtlType::Integral { width: *slice_total_w, signed: false, four_state: tf };
                let one = self.builder.alloc_const(CVal::new(1, false, 1), false, loc);
                let new_full = self
                    .apply_steps_to_obj_val(ObjVal::Packed(old_sub), &steps[1..], rhs_val, one, &sub_ty, loc)
                    .as_packed()
                    .unwrap_or(old_sub);
                let new_sub = if *rhs_lo == 0 && *width == *slice_total_w {
                    new_full
                } else {
                    let full_r = self.builder.resize_val(new_full, *slice_total_w, false, false, loc);
                    let (_, _, rf) = self.builder.node_shape(full_r);
                    self.builder.alloc_op(
                        NetOp::Slice { hi: rhs_lo + width - 1, lo: *rhs_lo },
                        vec![full_r],
                        *width,
                        false,
                        rf,
                        loc,
                    )
                };
                let replaced = replace_const_packed_slice(&mut self.builder, old_v, new_sub, *lo, *width, loc);
                ObjVal::Packed(self.builder.alloc_op(NetOp::Mux, vec![reach, replaced, old_v], tw, ts, tf, loc))
            }
            (ObjVal::Packed(old_v), LhsStep::PackedDynSlice { in_bounds, bit_lo, slice_w }) => {
                let Some(rv) = rhs_val.as_packed() else { return ObjVal::Packed(old_v) };
                let write_en = self.builder.alloc_and1(reach, *in_bounds, loc);
                ObjVal::Packed(rmw_dynamic_packed_slice(&mut self.builder, old_v, rv, *bit_lo, *slice_w, write_en, loc))
            }
            (
                ObjVal::Packed(old_v),
                LhsStep::PackedIndexedSlice { idx_val, idx_signed, bounds, elem_w, count, ascending },
            ) => {
                let Some(rv) = rhs_val.as_packed() else { return ObjVal::Packed(old_v) };
                let (total_w, _, _) = self.builder.node_shape(old_v);
                let slice_total_w = (*count) * (*elem_w);
                let rhs_sized = self.builder.resize_val(rv, slice_total_w, false, false, loc);
                let (_, _, rf) = self.builder.node_shape(rhs_sized);
                let (iw, _, if4) = self.builder.node_shape(*idx_val);
                let calc_w = iw.max(32).max(proc_addr_bits(total_w)) + 2;
                let ext_idx = self.builder.resize_val(*idx_val, calc_w, *idx_signed, true, loc);
                let lo_sv = if *ascending {
                    ext_idx
                } else {
                    let span_c = self.builder.alloc_const(CVal::new(calc_w, true, (*count - 1) as i128), false, loc);
                    self.builder.alloc_op(NetOp::Sub, vec![ext_idx, span_c], calc_w, true, if4, loc)
                };
                let mut acc = old_v;
                for k in 0..*count {
                    let sv_off = if bounds.0 >= bounds.1 { k } else { *count - 1 - k };
                    let lane_sv = if sv_off == 0 {
                        lo_sv
                    } else {
                        let kc = self.builder.alloc_const(CVal::new(calc_w, true, sv_off as i128), false, loc);
                        self.builder.alloc_op(NetOp::Add, vec![lo_sv, kc], calc_w, true, if4, loc)
                    };
                    let (in_b_k, bit_lo_k) =
                        proc_packed_offset(&mut self.builder, lane_sv, true, *bounds, *elem_w, 1, true, total_w, loc);
                    let rhs_k = self.builder.alloc_op(
                        NetOp::Slice { hi: (k + 1) * (*elem_w) - 1, lo: k * (*elem_w) },
                        vec![rhs_sized],
                        *elem_w,
                        false,
                        rf,
                        loc,
                    );
                    let write_en_k = self.builder.alloc_and1(reach, in_b_k, loc);
                    acc = rmw_dynamic_packed_slice(&mut self.builder, acc, rhs_k, bit_lo_k, *elem_w, write_en_k, loc);
                }
                ObjVal::Packed(acc)
            }
            (other, _) => other,
        }
    }

    pub(crate) fn read_symbol_current_in_proc(&mut self, slot: &ObjSlot, ctx: &ProcCtx, loc: Loc) -> ObjVal {
        match slot {
            ObjSlot::Packed(sidx) => {
                let v = ctx.slot_writes.get(sidx).map_or(self.slot_placeholders[*sidx], |sw| sw.cur_val);
                ObjVal::Packed(v)
            }
            ObjSlot::Const(cv, four) => ObjVal::Packed(self.builder.alloc_const(cv.clone(), *four, loc)),
            ObjSlot::Struct(fields) => ObjVal::Struct(
                fields.iter().map(|(n, fs)| (n.clone(), self.read_symbol_current_in_proc(fs, ctx, loc))).collect(),
            ),
            ObjSlot::Elems { left, right, elems } => ObjVal::Elems {
                left: *left,
                right: *right,
                elems: elems.iter().map(|es| self.read_symbol_current_in_proc(es, ctx, loc)).collect(),
            },
            ObjSlot::Memory(mid) => ObjVal::Memory(*mid),
        }
    }

    pub(crate) fn make_proc_binder<'b>(&'b self, ctx: &'b ProcCtx, arena: &'b mut ExprArena) -> ExprBinder<'b> {
        let mut binder =
            ExprBinder::new(self.trees, self.design, ctx.inst_id, &ctx.scope_prefix, &ctx.gen_consts, arena);
        binder.locals = ctx.local_decls.clone();
        binder.local_typedefs = ctx.local_typedefs.clone();
        binder
    }

    pub(crate) fn bind_in_proc(
        &mut self,
        tree_idx: usize,
        node: NodeId,
        ctx: &ProcCtx,
        arena: &mut ExprArena,
        as_cond: bool,
    ) -> Option<ExprId> {
        let mut binder = self.make_proc_binder(ctx, arena);
        let res = if as_cond { binder.bind_condition(tree_idx, node) } else { binder.bind_self(tree_idx, node) };
        self.flush_expr_diags(binder.diags);
        res
    }

    pub(crate) fn lower_in_proc(&mut self, arena: &ExprArena, eid: ExprId, ctx: &ProcCtx) -> Option<ObjVal> {
        let sym_slots = self.sym_slots.clone();
        let placeholders = self.slot_placeholders.clone();
        let config = self.config;
        let mut builder = std::mem::replace(&mut self.builder, NetBuilder::new(String::new(), Vec::new()));
        let mut lowerer = ExprLowerer {
            arena,
            config,
            active_clock: ctx.active_clock,
            sym_slots: &sym_slots,
            slot_placeholders: &placeholders,
            cur_syms: &ctx.cur_syms,
            cur_locals: &ctx.cur_locals,
            inliner: self,
        };
        let res = lowerer.lower_any(&mut builder, eid);
        self.builder = builder;
        res
    }

    pub(crate) fn eval_proc_cond(&mut self, tree_idx: usize, node: NodeId, ctx: &ProcCtx) -> Option<ValueId> {
        let mut arena = ExprArena::new();
        let eid = self.bind_in_proc(tree_idx, node, ctx, &mut arena, true)?;
        let loc = arena.get(eid).loc;
        let pv = self.lower_in_proc(&arena, eid, ctx)?.as_packed()?;
        Some(self.builder.alloc_bool(pv, loc))
    }

    pub(crate) fn stmt_loc(&mut self, tree_idx: usize, node: NodeId, ctx: &ProcCtx) -> Loc {
        let mut arena = ExprArena::new();
        let mut binder = self.make_proc_binder(ctx, &mut arena);
        binder.loc_of(tree_idx, node)
    }

    pub(crate) fn has_embedded_timing_control(&self, tree_idx: usize, root: NodeId) -> bool {
        let t = &self.trees[tree_idx];
        for n in subtree_preorder(t, root) {
            if matches!(
                t.kind(n),
                SyntaxKind::TimedStmt | SyntaxKind::WaitStmt | SyntaxKind::TimedExpr | SyntaxKind::EventTrigger
            ) {
                return true;
            }
        }
        false
    }
}

pub(crate) fn own_toks(t: &Tree, node: NodeId) -> Vec<&str> {
    t.node(node)
        .children
        .iter()
        .filter_map(|c| match *c {
            Child::Token(i) if !t.tokens[i as usize].kind.is_trivia() => Some(&*t.tokens[i as usize].text),
            _ => None,
        })
        .collect()
}

fn has_edge_kw(t: &Tree, ev: NodeId) -> bool {
    own_toks(t, ev).iter().any(|k| matches!(*k, "posedge" | "negedge" | "edge"))
}

fn event_edge(t: &Tree, ev: NodeId) -> Option<Edge> {
    let toks = own_toks(t, ev);
    if toks.contains(&"posedge") {
        Some(Edge::Posedge)
    } else if toks.contains(&"negedge") {
        Some(Edge::Negedge)
    } else {
        None
    }
}

fn extract_root_symbol(arena: &ExprArena, mut eid: ExprId) -> Option<ResetTarget> {
    loop {
        match &arena.get(eid).kind {
            ExprKind::Symbol(sid) => return Some(ResetTarget::Symbol(*sid)),
            ExprKind::Member { base, field, .. } => {
                let inner = extract_root_symbol(arena, *base)?;
                return Some(ResetTarget::Member { base: Box::new(inner), field: field.clone() });
            }
            ExprKind::BitSelect { base, index, .. } => {
                let icv = arena.get(*index).cval.as_ref()?;
                if !icv.known() {
                    return None;
                }
                let inner = extract_root_symbol(arena, *base)?;
                return Some(ResetTarget::Bit { base: Box::new(inner), index: icv.i() });
            }
            ExprKind::Unary { op: UnaryOp::RedOr, arg }
            | ExprKind::Resize { arg, .. }
            | ExprKind::CastSign { arg, .. }
            | ExprKind::CastFourState { arg, .. } => eid = *arg,
            _ => return None,
        }
    }
}

fn unwrap_single_if_stmt(t: &Tree, mut stmt: NodeId) -> Option<NodeId> {
    while t.kind(stmt) == SyntaxKind::SeqBlock {
        let stmts: Vec<NodeId> = t
            .children(stmt)
            .filter(|&c| !matches!(t.kind(c), SyntaxKind::Attribute | SyntaxKind::StmtLabel | SyntaxKind::Name))
            .collect();
        if stmts.len() == 1 {
            stmt = stmts[0];
        } else {
            return None;
        }
    }
    (t.kind(stmt) == SyntaxKind::IfStmt).then_some(stmt)
}

fn top_if_else_stmt(t: &Tree, if_node: NodeId, kids: &[NodeId]) -> Option<NodeId> {
    if let Some(ec) = t.child(if_node, SyntaxKind::ElseClause) {
        return t.children(ec).find(|&c| t.kind(c) != SyntaxKind::Attribute);
    }
    kids.get(2).copied().filter(|&c| t.kind(c) != SyntaxKind::ElseClause)
}

/// Checks whether `cond_eid` is an active-high (`posedge rst` with `if (rst)`) or active-low
/// (`negedge rst_n` with `if (!rst_n)` / `if (rst_n == 0)`) test of one of `events`, returning
/// `(rst_event_index, active_high)`.
fn match_async_reset_event(
    arena: &ExprArena,
    cond_eid: ExprId,
    events: &[(Edge, ValueId, Option<ResetTarget>, Loc)],
) -> Option<(usize, bool)> {
    let (tested_target, cond_active_high) = analyze_reset_condition(arena, cond_eid)?;
    // Prefer matching `events[1]` first (`@(posedge clk or negedge rst_n)`), then `events[0]`.
    for &idx in &[1usize, 0usize] {
        if let Some((edge, _, Some(ev_target), _)) = events.get(idx) {
            let edge_high = *edge == Edge::Posedge;
            if *ev_target == tested_target && edge_high == cond_active_high {
                return Some((idx, cond_active_high));
            }
        }
    }
    None
}

fn analyze_reset_condition(arena: &ExprArena, mut eid: ExprId) -> Option<(ResetTarget, bool)> {
    let mut active_high = true;
    loop {
        match &arena.get(eid).kind {
            ExprKind::Symbol(_) | ExprKind::Member { .. } | ExprKind::BitSelect { .. } => {
                if arena.get(eid).ty.bit_width() != Some(1) {
                    return None;
                }
                return Some((extract_root_symbol(arena, eid)?, active_high));
            }
            ExprKind::Unary { op: UnaryOp::LogNot | UnaryOp::BitNot, arg } => {
                active_high = !active_high;
                eid = *arg;
            }
            ExprKind::Unary { op: UnaryOp::RedOr, arg }
            | ExprKind::Resize { arg, .. }
            | ExprKind::CastSign { arg, .. }
            | ExprKind::CastFourState { arg, .. } => {
                eid = *arg;
            }
            ExprKind::Binary { op: BinaryOp::Eq | BinaryOp::Neq | BinaryOp::CaseEq | BinaryOp::CaseNeq, lhs, rhs } => {
                let op = match &arena.get(eid).kind {
                    ExprKind::Binary { op, .. } => *op,
                    _ => unreachable!(),
                };
                let is_eq = matches!(op, BinaryOp::Eq | BinaryOp::CaseEq);
                if let Some(rc) = &arena.get(*rhs).cval {
                    if rc.known() {
                        let rhs_nonzero = match rc.i() {
                            0 => false,
                            1 | -1 => true,
                            _ => return None,
                        };
                        let branch_high = if is_eq { rhs_nonzero } else { !rhs_nonzero };
                        if !branch_high {
                            active_high = !active_high;
                        }
                        eid = *lhs;
                        continue;
                    }
                }
                if let Some(lc) = &arena.get(*lhs).cval {
                    if lc.known() {
                        let lhs_nonzero = match lc.i() {
                            0 => false,
                            1 | -1 => true,
                            _ => return None,
                        };
                        let branch_high = if is_eq { lhs_nonzero } else { !lhs_nonzero };
                        if !branch_high {
                            active_high = !active_high;
                        }
                        eid = *rhs;
                        continue;
                    }
                }
                return None;
            }
            _ => return None,
        }
    }
}

#[allow(clippy::type_complexity)]
fn parse_case_branches(t: &Tree, stmt: NodeId) -> Option<(Option<NodeId>, Vec<(Vec<NodeId>, NodeId)>, Option<NodeId>)> {
    let mut sel_node: Option<NodeId> = None;
    let mut branches: Vec<(Vec<NodeId>, NodeId)> = Vec::new();
    let mut default_stmt: Option<NodeId> = None;
    let mut cur_pats: Vec<NodeId> = Vec::new();
    let mut in_default = false;
    let mut after_colon = false;

    for ch in &t.node(stmt).children {
        match *ch {
            Child::Token(ti) => {
                let tok = &t.tokens[ti as usize];
                if tok.kind.is_trivia() {
                    continue;
                }
                match &*tok.text {
                    "default" => {
                        in_default = true;
                        // Note: `default` may omit `:` in SV (`default y = 0;`)!
                        after_colon = true;
                    }
                    ":" => after_colon = true,
                    _ => {}
                }
            }
            Child::Node(n) => {
                if matches!(t.kind(n), SyntaxKind::Attribute | SyntaxKind::StmtLabel) {
                    continue;
                }
                if sel_node.is_none() {
                    sel_node = Some(n);
                    continue;
                }
                if after_colon {
                    if in_default {
                        default_stmt = Some(n);
                    } else {
                        branches.push((std::mem::take(&mut cur_pats), n));
                    }
                    in_default = false;
                    after_colon = false;
                } else {
                    cur_pats.push(n);
                }
            }
        }
    }
    Some((sel_node, branches, default_stmt))
}

fn contiguous_runs(mask: &[bool]) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < mask.len() {
        if !mask[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i + 1 < mask.len() && mask[i + 1] {
            i += 1;
        }
        out.push((start as u32, i as u32));
        i += 1;
    }
    out
}

fn contiguous_runs_with_tag(mask: &[bool], tag: &[bool]) -> Vec<(u32, u32, bool)> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < mask.len() {
        if !mask[i] {
            i += 1;
            continue;
        }
        let start = i;
        let t = tag[i];
        while i + 1 < mask.len() && mask[i + 1] && tag[i + 1] == t {
            i += 1;
        }
        out.push((start as u32, i as u32, t));
        i += 1;
    }
    out
}

pub(crate) fn is_assign_op(op: &str) -> bool {
    matches!(op, "=" | "<=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=" | "<<<=" | ">>>=")
}
