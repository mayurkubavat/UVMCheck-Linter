//! Static loop unrolling (`for`, `while`, `do...while`, `repeat`, `foreach`),
//! timing-free `function` and `task` inlining, procedural local declarations,
//! and `initial`-block constant extraction.

use sv_semantic::expr::{extract_subroutine_formals, subroutine_decl_name, SubroutineFormal};
use sv_semantic::{CVal, ElabItem, ElabItemKind, ExprArena, InstanceId, Loc, LocalBinding, LocalId, PortDir, RtlType};
use sv_syntax::{Child, NodeId, SyntaxKind, Tree};

use crate::builder::NetBuilder;
use crate::expr_lower::{make_oob_obj_val, mux_obj_val, FuncInliner, ObjVal};
use crate::lower::{own_first_tok, DesignLowerer};
use crate::proc::{own_toks, ProcCtx, ProcKind};

impl<'a> FuncInliner for DesignLowerer<'a> {
    fn inline_func_call(
        &mut self,
        builder: &mut NetBuilder,
        callee: (usize, NodeId),
        callee_inst: Option<InstanceId>,
        callee_scope_prefix: &str,
        name: &str,
        args: &[ObjVal],
        ret_ty: &RtlType,
        loc: Loc,
    ) -> Option<ObjVal> {
        std::mem::swap(&mut self.builder, builder);
        let res = self.run_inline_function(callee, callee_inst, callee_scope_prefix, name, args, ret_ty, loc);
        std::mem::swap(&mut self.builder, builder);
        res
    }
}

impl<'a> DesignLowerer<'a> {
    pub(crate) fn alloc_local_id(&mut self) -> LocalId {
        let id = LocalId(self.next_local_id);
        self.next_local_id += 1;
        id
    }

    pub(crate) fn exec_local_decl(&mut self, tree_idx: usize, stmt: NodeId, ctx: &mut ProcCtx) {
        let _ = self.exec_local_decl_with_fallback(tree_idx, stmt, None, ctx);
    }

    fn exec_local_decl_with_fallback(
        &mut self,
        tree_idx: usize,
        stmt: NodeId,
        fallback_dt: Option<NodeId>,
        ctx: &mut ProcCtx,
    ) -> Option<NodeId> {
        let t = &self.trees[tree_idx];
        let own = own_toks(t, stmt);
        // Inside non-ANSI subroutine bodies, port declarations (`input a;`, `output b;`) were already bound as formals.
        if t.kind(stmt) == SyntaxKind::DataDecl
            && own.first().is_some_and(|kw| matches!(*kw, "input" | "output" | "inout" | "ref"))
        {
            return None;
        }

        let explicit_dt = t.child(stmt, SyntaxKind::DataType).or(fallback_dt);
        let decls: Vec<NodeId> = t.children(stmt).filter(|&c| t.kind(c) == SyntaxKind::Declarator).collect();
        let loc = self.stmt_loc(tree_idx, stmt, ctx);

        for decl in decls {
            let Some(nm_node) = t.child(decl, SyntaxKind::Name) else { continue };
            let Some(&tok) = self.trees[tree_idx].token_range(nm_node).last() else { continue };
            let raw = &*self.trees[tree_idx].tokens[tok as usize].text;
            let name = raw.strip_prefix('\\').unwrap_or(raw).to_string();
            let init_node = self.trees[tree_idx].children(decl).find(|&c| {
                !matches!(self.trees[tree_idx].kind(c), SyntaxKind::Name | SyntaxKind::Dim | SyntaxKind::Attribute)
            });

            let (ty, packed_bounds) = {
                let mut arena = ExprArena::new();
                let mut binder = self.make_proc_binder(ctx, &mut arena);
                let resolved_ty = if self.trees[tree_idx].kind(stmt) == SyntaxKind::ParamDecl && explicit_dt.is_none() {
                    if let Some(in_n) = init_node {
                        binder
                            .bind_self(tree_idx, in_n)
                            .map(|e| binder.arena.get(e).ty.clone())
                            .unwrap_or(RtlType::Integral { width: 32, signed: true, four_state: true })
                    } else {
                        RtlType::Integral { width: 32, signed: true, four_state: true }
                    }
                } else {
                    let Some(ty) = binder.resolve_subroutine_dtype(
                        tree_idx,
                        explicit_dt,
                        Some(decl),
                        Some(ctx.inst_id),
                        &ctx.scope_prefix,
                    ) else {
                        let diags = binder.diags;
                        self.flush_expr_diags(diags);
                        continue;
                    };
                    ty
                };
                // In `for (i = 0)` vs `for (int i = 0, j = 1)`, bare `DataDecl` with no `DataType` defaults to `logic` unless `ParamDecl`.
                if let RtlType::NonSynthesizable(reason) = &resolved_ty {
                    let diags = binder.diags;
                    self.flush_expr_diags(diags);
                    self.builder.err(loc, format!("local variable '{name}' has non-synthesizable type '{reason}'"));
                    continue;
                }
                let pbounds = explicit_dt.and_then(|dt| {
                    let pdims: Vec<NodeId> = self.trees[tree_idx]
                        .children(dt)
                        .filter(|&c| self.trees[tree_idx].kind(c) == SyntaxKind::Dim)
                        .collect();
                    let integral_leaf = matches!(resolved_ty, RtlType::Integral { .. })
                        || matches!(&resolved_ty, RtlType::UnpackedArray { elem, .. } if matches!(**elem, RtlType::Integral { .. }));
                    if pdims.len() == 1 && integral_leaf {
                        binder.eval_dim_bounds(tree_idx, pdims[0])
                    } else {
                        None
                    }
                });
                let diags = binder.diags;
                self.flush_expr_diags(diags);
                if !crate::lower::validate_rtl_type(&mut self.builder, &name, &resolved_ty, loc) {
                    continue;
                }
                (resolved_ty, pbounds)
            };

            let lid = self.alloc_local_id();
            let (init_val, init_cval) = if let Some(in_n) = init_node {
                let mut arena = ExprArena::new();
                let mut binder = self.make_proc_binder(ctx, &mut arena);
                let eid_opt = binder.bind_assigned(tree_idx, in_n, &ty);
                self.flush_expr_diags(binder.diags);
                if let Some(eid) = eid_opt {
                    let bound_cval = arena.get(eid).cval.clone();
                    if let Some(ov) = self.lower_in_proc(&arena, eid, ctx) {
                        let cv =
                            bound_cval.or_else(|| ov.as_packed().and_then(|v| self.builder.const_cval(v).cloned()));
                        (ov, cv)
                    } else {
                        (make_oob_obj_val(&mut self.builder, &ty, loc), None)
                    }
                } else {
                    (make_oob_obj_val(&mut self.builder, &ty, loc), None)
                }
            } else {
                (make_oob_obj_val(&mut self.builder, &ty, loc), None)
            };

            // Guard write to local if declared inside a conditional branch? For a fresh `lid`, `init_val` IS its value!
            ctx.cur_locals.insert(lid, init_val);
            ctx.local_decls.insert(name, LocalBinding { id: lid, ty, packed_bounds, cval: init_cval });
        }

        explicit_dt
    }

    pub(crate) fn exec_local_typedef(&mut self, tree_idx: usize, stmt: NodeId, ctx: &mut ProcCtx) {
        let t = &self.trees[tree_idx];
        let Some(nm_node) = t.child(stmt, SyntaxKind::Name) else { return };
        let Some(&tok) = t.token_range(nm_node).last() else { return };
        let raw = &*t.tokens[tok as usize].text;
        let name = raw.strip_prefix('\\').unwrap_or(raw).to_string();
        let Some(dt) = t.child(stmt, SyntaxKind::DataType) else { return };

        let mut arena = ExprArena::new();
        let mut binder = self.make_proc_binder(ctx, &mut arena);
        let ty_opt = binder.eval_simple_dtype(tree_idx, dt, Some(ctx.inst_id), &ctx.scope_prefix);
        self.flush_expr_diags(binder.diags);
        if let Some(ty) = ty_opt {
            ctx.local_typedefs.insert(name, ty);
        }
    }

    pub(crate) fn exec_loop_stmt(&mut self, tree_idx: usize, stmt: NodeId, ctx: &mut ProcCtx) {
        let t = &self.trees[tree_idx];
        let loc = self.stmt_loc(tree_idx, stmt, ctx);
        ctx.loop_depth += 1;
        if t.kind(stmt) == SyntaxKind::DoWhileStmt {
            self.exec_do_while_loop(tree_idx, stmt, ctx, loc);
            ctx.loop_depth -= 1;
            return;
        }
        let kw = own_first_tok(t, stmt).unwrap_or("");
        match kw {
            "for" => self.exec_for_loop(tree_idx, stmt, ctx, loc),
            "while" => self.exec_while_loop(tree_idx, stmt, ctx, loc),
            "repeat" => self.exec_repeat_loop(tree_idx, stmt, ctx, loc),
            "foreach" => self.exec_foreach_loop(tree_idx, stmt, ctx, loc),
            "forever" => self.exec_forever_loop(tree_idx, stmt, ctx, loc),
            _ => {
                self.builder.unsup(loc, format!("unsupported loop statement '{kw}'"));
            }
        }
        ctx.loop_depth -= 1;
    }

    fn exec_for_loop(&mut self, tree_idx: usize, stmt: NodeId, ctx: &mut ProcCtx, loc: Loc) {
        let (inits, cond_opt, steps, Some(body)) = split_for_header(&self.trees[tree_idx], stmt) else {
            return;
        };
        let saved_decls = ctx.local_decls.clone();
        let saved_tdefs = ctx.local_typedefs.clone();
        let saved_break = ctx.break_cond;
        let saved_cont = ctx.continue_cond;
        let zero = self.builder.alloc_const(CVal::new(1, false, 0), false, loc);
        ctx.break_cond = zero;
        ctx.continue_cond = zero;

        let mut last_dt: Option<NodeId> = None;
        for init_n in inits {
            if self.trees[tree_idx].kind(init_n) == SyntaxKind::DataDecl {
                last_dt = self.exec_local_decl_with_fallback(tree_idx, init_n, last_dt, ctx);
            } else {
                self.exec_expr_stmt(tree_idx, init_n, ctx);
            }
        }
        // Any loop variable freshly declared in `for (int i = 0; ...)` goes out of scope on exit,
        // so advancing it unconditionally in `steps` preserves constant unrolling across dynamic `break`s!
        let header_lids: Vec<LocalId> = ctx
            .local_decls
            .iter()
            .filter_map(|(k, lb)| (saved_decls.get(k).map(|old| old.id) != Some(lb.id)).then_some(lb.id))
            .collect();

        let max_iters = self.config.max_loop_iters;
        let mut iter = 0usize;
        loop {
            if let Some(cond_n) = cond_opt {
                let Some(go) = self.eval_static_loop_cond(tree_idx, cond_n, ctx, loc) else {
                    break;
                };
                if !go {
                    break;
                }
            }
            if iter >= max_iters {
                self.builder.unsup(loc, format!("loop exceeded max_loop_iters ({max_iters})"));
                break;
            }
            self.exec_stmt(tree_idx, body, ctx);
            ctx.continue_cond = zero;
            if self.is_const_one(ctx.break_cond) || self.is_const_one(ctx.returned_cond) {
                break;
            }
            let prev_uncond = std::mem::replace(&mut ctx.unconditional_lids, header_lids.clone());
            for &step_n in &steps {
                self.exec_expr_stmt(tree_idx, step_n, ctx);
            }
            ctx.unconditional_lids = prev_uncond;
            iter += 1;
        }

        ctx.break_cond = saved_break;
        ctx.continue_cond = saved_cont;
        ctx.restore_scope(&self.builder, saved_decls, saved_tdefs);
    }

    fn exec_while_loop(&mut self, tree_idx: usize, stmt: NodeId, ctx: &mut ProcCtx, loc: Loc) {
        let kids: Vec<NodeId> = self.trees[tree_idx]
            .children(stmt)
            .filter(|&c| !matches!(self.trees[tree_idx].kind(c), SyntaxKind::Attribute | SyntaxKind::StmtLabel))
            .collect();
        let (Some(&cond_n), Some(&body)) = (kids.first(), kids.get(1)) else { return };
        let saved_break = ctx.break_cond;
        let saved_cont = ctx.continue_cond;
        let zero = self.builder.alloc_const(CVal::new(1, false, 0), false, loc);
        ctx.break_cond = zero;
        ctx.continue_cond = zero;

        let max_iters = self.config.max_loop_iters;
        let mut iter = 0usize;
        while let Some(go) = self.eval_static_loop_cond(tree_idx, cond_n, ctx, loc) {
            if !go {
                break;
            }
            if iter >= max_iters {
                self.builder.unsup(loc, format!("while loop exceeded max_loop_iters ({max_iters})"));
                break;
            }
            self.exec_stmt(tree_idx, body, ctx);
            ctx.continue_cond = zero;
            if self.is_const_one(ctx.break_cond) || self.is_const_one(ctx.returned_cond) {
                break;
            }
            iter += 1;
        }

        ctx.break_cond = saved_break;
        ctx.continue_cond = saved_cont;
    }

    fn exec_do_while_loop(&mut self, tree_idx: usize, stmt: NodeId, ctx: &mut ProcCtx, loc: Loc) {
        let kids: Vec<NodeId> = self.trees[tree_idx]
            .children(stmt)
            .filter(|&c| !matches!(self.trees[tree_idx].kind(c), SyntaxKind::Attribute | SyntaxKind::StmtLabel))
            .collect();
        let (Some(&body), Some(&cond_n)) = (kids.first(), kids.get(1)) else { return };
        let saved_break = ctx.break_cond;
        let saved_cont = ctx.continue_cond;
        let zero = self.builder.alloc_const(CVal::new(1, false, 0), false, loc);
        ctx.break_cond = zero;
        ctx.continue_cond = zero;

        let max_iters = self.config.max_loop_iters;
        let mut iter = 0usize;
        loop {
            if iter >= max_iters {
                self.builder.unsup(loc, format!("do...while loop exceeded max_loop_iters ({max_iters})"));
                break;
            }
            self.exec_stmt(tree_idx, body, ctx);
            ctx.continue_cond = zero;
            if self.is_const_one(ctx.break_cond) || self.is_const_one(ctx.returned_cond) {
                break;
            }
            let Some(go) = self.eval_static_loop_cond(tree_idx, cond_n, ctx, loc) else { break };
            if !go {
                break;
            }
            iter += 1;
        }

        ctx.break_cond = saved_break;
        ctx.continue_cond = saved_cont;
    }

    fn exec_repeat_loop(&mut self, tree_idx: usize, stmt: NodeId, ctx: &mut ProcCtx, loc: Loc) {
        let kids: Vec<NodeId> = self.trees[tree_idx]
            .children(stmt)
            .filter(|&c| !matches!(self.trees[tree_idx].kind(c), SyntaxKind::Attribute | SyntaxKind::StmtLabel))
            .collect();
        let (Some(&cnt_n), Some(&body)) = (kids.first(), kids.get(1)) else { return };

        let mut arena = ExprArena::new();
        let Some(eid) = self.bind_in_proc(tree_idx, cnt_n, ctx, &mut arena, false) else { return };
        let cval_opt = arena.get(eid).cval.clone().or_else(|| {
            self.lower_in_proc(&arena, eid, ctx)?.as_packed().and_then(|v| self.builder.const_cval(v).cloned())
        });
        let Some(cv) = cval_opt else {
            self.builder.unsup(loc, "repeat loop count is not an elaboration/unroll-time constant");
            return;
        };
        if !cv.known() || (cv.s && cv.i() <= 0) || (!cv.s && cv.i() == 0) {
            // IEEE 1800 §12.7.2: if repeat count is <= 0 or X/Z, execute 0 times.
            return;
        }
        let n = cv.i() as usize;
        if n > self.config.max_loop_iters {
            self.builder
                .unsup(loc, format!("repeat count ({n}) exceeds max_loop_iters ({})", self.config.max_loop_iters));
            return;
        }

        let saved_break = ctx.break_cond;
        let saved_cont = ctx.continue_cond;
        let zero = self.builder.alloc_const(CVal::new(1, false, 0), false, loc);
        ctx.break_cond = zero;
        ctx.continue_cond = zero;

        for _ in 0..n {
            self.exec_stmt(tree_idx, body, ctx);
            ctx.continue_cond = zero;
            if self.is_const_one(ctx.break_cond) || self.is_const_one(ctx.returned_cond) {
                break;
            }
        }

        ctx.break_cond = saved_break;
        ctx.continue_cond = saved_cont;
    }

    fn exec_forever_loop(&mut self, tree_idx: usize, stmt: NodeId, ctx: &mut ProcCtx, loc: Loc) {
        let Some(body) = self.trees[tree_idx]
            .children(stmt)
            .find(|&c| !matches!(self.trees[tree_idx].kind(c), SyntaxKind::Attribute | SyntaxKind::StmtLabel))
        else {
            return;
        };
        let saved_break = ctx.break_cond;
        let saved_cont = ctx.continue_cond;
        let zero = self.builder.alloc_const(CVal::new(1, false, 0), false, loc);
        ctx.break_cond = zero;
        ctx.continue_cond = zero;

        let max_iters = self.config.max_loop_iters;
        let mut exited = false;
        for _ in 0..max_iters {
            self.exec_stmt(tree_idx, body, ctx);
            ctx.continue_cond = zero;
            if self.is_const_one(ctx.break_cond) || self.is_const_one(ctx.returned_cond) {
                exited = true;
                break;
            }
        }
        if !exited {
            self.builder.unsup(loc, "unbounded 'forever' loop is not synthesizable in RTL");
        }

        ctx.break_cond = saved_break;
        ctx.continue_cond = saved_cont;
    }

    fn exec_foreach_loop(&mut self, tree_idx: usize, stmt: NodeId, ctx: &mut ProcCtx, loc: Loc) {
        let t = &self.trees[tree_idx];
        let Some(body) =
            t.children(stmt).find(|&c| !matches!(t.kind(c), SyntaxKind::Attribute | SyntaxKind::StmtLabel))
        else {
            return;
        };
        let Some((arr_name, vars)) = parse_foreach_header(t, stmt) else {
            self.builder.err(loc, "malformed 'foreach' header");
            return;
        };
        if vars.is_empty() {
            return;
        }
        let Some(dims) = self.resolve_foreach_dims(&arr_name, vars.len(), ctx, loc) else {
            return;
        };

        let saved_decls = ctx.local_decls.clone();
        let saved_tdefs = ctx.local_typedefs.clone();
        let saved_break = ctx.break_cond;
        let saved_cont = ctx.continue_cond;
        let zero = self.builder.alloc_const(CVal::new(1, false, 0), false, loc);
        ctx.break_cond = zero;
        ctx.continue_cond = zero;

        // Allocate one `LocalId` per named loop variable.
        let var_lids: Vec<Option<(String, LocalId)>> =
            vars.into_iter().map(|vo| vo.map(|nm| (nm, self.alloc_local_id()))).collect();

        let mut total_iters = 0usize;
        self.run_foreach_dim(tree_idx, body, &dims, &var_lids, 0, &mut total_iters, ctx, loc);

        ctx.break_cond = saved_break;
        ctx.continue_cond = saved_cont;
        ctx.restore_scope(&self.builder, saved_decls, saved_tdefs);
    }

    #[allow(clippy::too_many_arguments)]
    fn run_foreach_dim(
        &mut self,
        tree_idx: usize,
        body: NodeId,
        dims: &[(i128, i128)],
        var_lids: &[Option<(String, LocalId)>],
        depth: usize,
        total_iters: &mut usize,
        ctx: &mut ProcCtx,
        loc: Loc,
    ) {
        if depth >= dims.len() {
            if *total_iters >= self.config.max_loop_iters {
                if *total_iters == self.config.max_loop_iters {
                    self.builder
                        .unsup(loc, format!("foreach loop exceeded max_loop_iters ({})", self.config.max_loop_iters));
                    *total_iters += 1;
                }
                return;
            }
            *total_iters += 1;
            let zero = self.builder.alloc_const(CVal::new(1, false, 0), false, loc);
            self.exec_stmt(tree_idx, body, ctx);
            ctx.continue_cond = zero;
            return;
        }

        let (left, right) = dims[depth];
        let step: i128 = if left <= right { 1 } else { -1 };
        let count = ((left - right).unsigned_abs() as usize) + 1;
        let idx_ty = RtlType::Integral { width: 32, signed: true, four_state: false };

        for k in 0..count {
            if self.is_const_one(ctx.break_cond) || self.is_const_one(ctx.returned_cond) {
                break;
            }
            let cur_i = left + (k as i128) * step;
            if let Some((nm, lid)) = &var_lids[depth] {
                let cv = CVal::new(32, true, cur_i);
                let v = self.builder.alloc_const(cv.clone(), false, loc);
                ctx.cur_locals.insert(*lid, ObjVal::Packed(v));
                ctx.local_decls.insert(
                    nm.clone(),
                    LocalBinding { id: *lid, ty: idx_ty.clone(), packed_bounds: None, cval: Some(cv) },
                );
            }
            self.run_foreach_dim(tree_idx, body, dims, var_lids, depth + 1, total_iters, ctx, loc);
        }
    }

    fn resolve_foreach_dims(
        &mut self,
        arr_name: &str,
        num_vars: usize,
        ctx: &ProcCtx,
        loc: Loc,
    ) -> Option<Vec<(i128, i128)>> {
        let lookup_direct = |this: &Self, name: &str| -> Option<(RtlType, Option<(i128, i128)>)> {
            if let Some(lb) = ctx.local_decls.get(name) {
                Some((lb.ty.clone(), lb.packed_bounds))
            } else if let Some(sid) = this.design.instance(ctx.inst_id).resolve_lexical(&ctx.scope_prefix, name) {
                let sym = this.design.symbol(sid);
                Some((sym.ty.clone(), sym.packed_bounds))
            } else if let Some(sid) = this.design.resolve_hierarchical(ctx.inst_id, &ctx.scope_prefix, name) {
                let sym = this.design.symbol(sid);
                Some((sym.ty.clone(), sym.packed_bounds))
            } else {
                None
            }
        };

        let resolved = lookup_direct(self, arr_name).or_else(|| {
            let mut segs = arr_name.split('.');
            let first = segs.next()?;
            let (mut cur_ty, mut cur_pb) = lookup_direct(self, first)?;
            for field in segs {
                cur_pb = None;
                cur_ty = match cur_ty {
                    RtlType::UnpackedStruct { fields, .. } => fields.into_iter().find(|(n, _)| n == field)?.1,
                    RtlType::PackedStruct { fields, .. } | RtlType::PackedUnion { fields, .. } => {
                        fields.into_iter().find(|f| f.name == field)?.ty
                    }
                    _ => return None,
                };
            }
            Some((cur_ty, cur_pb))
        });

        let Some((ty, packed_1d)) = resolved else {
            self.builder.err(loc, format!("undeclared array '{arr_name}' in foreach loop"));
            return None;
        };

        let mut all_dims = Vec::new();
        collect_all_array_dims(&ty, packed_1d, &mut all_dims);
        if all_dims.len() < num_vars {
            self.builder.err(
                loc,
                format!(
                    "foreach specifies {num_vars} loop variables, but '{arr_name}' has {} dimension(s)",
                    all_dims.len()
                ),
            );
            return None;
        }
        all_dims.truncate(num_vars);
        Some(all_dims)
    }

    fn eval_static_loop_cond(&mut self, tree_idx: usize, cond_n: NodeId, ctx: &ProcCtx, loc: Loc) -> Option<bool> {
        let cv_val = self.eval_proc_cond(tree_idx, cond_n, ctx)?;
        let Some(cv) = self.builder.const_cval(cv_val).cloned() else {
            self.builder.unsup(loc, "loop termination condition is not an elaboration/unroll-time constant");
            return None;
        };
        // IEEE 1800: X/Z condition evaluates false in loop header.
        Some(cv.known() && cv.is_true())
    }

    pub(crate) fn is_const_one(&self, v: crate::ir::ValueId) -> bool {
        self.builder.const_cval(v).is_some_and(|c| c.known() && c.is_true())
    }

    #[allow(clippy::too_many_arguments)]
    fn run_inline_function(
        &mut self,
        callee: (usize, NodeId),
        callee_inst: Option<InstanceId>,
        callee_scope_prefix: &str,
        name: &str,
        args: &[ObjVal],
        ret_ty: &RtlType,
        loc: Loc,
    ) -> Option<ObjVal> {
        if self.call_stack.contains(&callee) {
            self.builder.err(loc, format!("recursive function call to '{name}' is not synthesizable in RTL"));
            return None;
        }
        let (fn_tree, fn_node) = callee;
        let Some(body) = self.trees[fn_tree].child(fn_node, SyntaxKind::Body) else {
            return Some(make_oob_obj_val(&mut self.builder, ret_ty, loc));
        };
        if self.has_embedded_timing_control(fn_tree, body) {
            self.builder.unsup(loc, format!("function '{name}' contains timing control and cannot be inlined"));
            return None;
        }

        let owner_inst = callee_inst.unwrap_or_else(|| self.design.tops[0]);
        let gen_consts = self.collect_scope_gen_consts(owner_inst, callee_scope_prefix);
        let mut sub_ctx = ProcCtx::new(
            &mut self.builder,
            owner_inst,
            callee_scope_prefix.to_string(),
            gen_consts,
            ProcKind::Subroutine,
            None,
            loc,
        );

        // Allocate return local (also registered under `name` so `fn_name = expr;` works alongside `return expr;`).
        let ret_lid = self.alloc_local_id();
        let def_ret = make_oob_obj_val(&mut self.builder, ret_ty, loc);
        sub_ctx.cur_locals.insert(ret_lid, def_ret.clone());
        sub_ctx.ret_local = Some((ret_lid, ret_ty.clone()));
        if *ret_ty != RtlType::Void {
            sub_ctx.local_decls.insert(
                name.to_string(),
                LocalBinding { id: ret_lid, ty: ret_ty.clone(), packed_bounds: None, cval: None },
            );
        }

        // Bind formal inputs; reject output/inout/ref formals on expression-called functions.
        let formals = extract_subroutine_formals(&self.trees[fn_tree], fn_node);
        for (i, formal) in formals.iter().enumerate() {
            if !matches!(formal.dir, PortDir::Input) {
                self.builder.unsup(
                    loc,
                    format!(
                        "function '{name}' with output/inout/ref argument '{}' cannot be called inside an expression",
                        formal.name
                    ),
                );
                return None;
            }
            let fty = ({
                let mut arena = ExprArena::new();
                let mut binder = self.make_proc_binder(&sub_ctx, &mut arena);
                let r = binder.resolve_subroutine_dtype(
                    fn_tree,
                    formal.dtype,
                    formal.decl,
                    callee_inst,
                    callee_scope_prefix,
                );
                self.flush_expr_diags(binder.diags);
                r
            })?;
            let flid = self.alloc_local_id();
            let aval = args.get(i).cloned().unwrap_or_else(|| make_oob_obj_val(&mut self.builder, &fty, loc));
            let cval = aval.as_packed().and_then(|v| self.builder.const_cval(v).cloned());
            sub_ctx.cur_locals.insert(flid, aval);
            sub_ctx
                .local_decls
                .insert(formal.name.clone(), LocalBinding { id: flid, ty: fty, packed_bounds: None, cval });
        }

        let checks_before = self.builder.checks.len();
        let mem_writes_before: Vec<usize> = self.builder.memories.iter().map(|m| m.writes.len()).collect();

        self.call_stack.push(callee);
        let items: Vec<NodeId> = self.trees[fn_tree]
            .children(body)
            .filter(|&c| self.trees[fn_tree].kind(c) != SyntaxKind::Attribute)
            .collect();
        for item_n in items {
            self.exec_stmt(fn_tree, item_n, &mut sub_ctx);
        }
        self.call_stack.pop();

        let wrote_mem = self.builder.memories.iter().enumerate().any(|(mi, m)| m.writes.len() > mem_writes_before[mi]);
        let added_checks = self.builder.checks.len() > checks_before;
        if !sub_ctx.slot_writes.is_empty() || added_checks || wrote_mem {
            self.builder.checks.truncate(checks_before);
            for (mi, &mw) in mem_writes_before.iter().enumerate() {
                self.builder.memories[mi].writes.truncate(mw);
            }
            self.builder
                .unsup(loc, format!("side-effecting function '{name}' called inside an expression is not supported"));
            return None;
        }

        Some(sub_ctx.cur_locals.remove(&ret_lid).unwrap_or(def_ret))
    }

    pub(crate) fn exec_call_stmt(&mut self, tree_idx: usize, expr_node: NodeId, ctx: &mut ProcCtx, loc: Loc) {
        let t = &self.trees[tree_idx];
        let (callee_node, arg_nodes) = if t.kind(expr_node) == SyntaxKind::CallExpr {
            let Some(cn) = t.children(expr_node).find(|&c| t.kind(c) != SyntaxKind::Attribute) else { return };
            (cn, call_stmt_arg_nodes(t, expr_node))
        } else {
            (expr_node, Vec::new())
        };

        // Simulation-only and non-synthesizable system tasks (`$display`, `$readmemh`, `$fopen`, `$random`, ...).
        if t.kind(callee_node) == SyntaxKind::NameExpr {
            let raw = t.bare_text(callee_node);
            let trimmed = raw.trim();
            if trimmed.starts_with('$') {
                match trimmed {
                    "$display" | "$displayb" | "$displayh" | "$displayo" | "$write" | "$writeb" | "$writeh"
                    | "$writeo" | "$strobe" | "$strobeb" | "$strobeh" | "$strobeo" | "$monitor" | "$monitorb"
                    | "$monitorh" | "$monitoro" | "$info" | "$warning" => {
                        if arg_nodes.iter().any(|&an| expr_subtree_has_side_effect(t, an)) {
                            self.builder
                                .unsup(loc, format!("side-effecting argument inside '{trimmed}' is not supported"));
                        }
                        return;
                    }
                    "$error" | "$fatal" => {
                        if arg_nodes.iter().any(|&an| expr_subtree_has_side_effect(t, an)) {
                            self.builder
                                .unsup(loc, format!("side-effecting argument inside '{trimmed}' is not supported"));
                            return;
                        }
                        let zero = self.builder.alloc_const(CVal::new(1, false, 0), false, loc);
                        let enable = ctx.effective_reach(&mut self.builder, loc);
                        self.builder.checks.push(crate::ir::Check {
                            kind: crate::ir::CheckKind::Assert,
                            label: None,
                            immediate: true,
                            enable,
                            expr: zero,
                            clock: ctx.active_clock,
                            loc,
                        });
                        return;
                    }
                    _ => {
                        self.builder.unsup(loc, format!("system task '{trimmed}' is not supported in RTL netlist"));
                        return;
                    }
                }
            }
        }

        let Some((sub_kind, sub_tree, sub_node, sub_inst, sub_prefix)) = crate::lower::resolve_call_target_in_scope(
            self.trees,
            self.design,
            ctx.inst_id,
            &ctx.scope_prefix,
            t,
            callee_node,
        ) else {
            self.builder.err(loc, format!("undeclared task or function '{}'", t.bare_text(callee_node).trim()));
            return;
        };
        let name = subroutine_decl_name(&self.trees[sub_tree], sub_node).unwrap_or_default();
        let callee = (sub_tree, sub_node);
        if self.call_stack.contains(&callee) {
            self.builder.err(loc, format!("recursive subroutine call to '{name}' is not synthesizable in RTL"));
            return;
        }
        let Some(body) = self.trees[sub_tree].child(sub_node, SyntaxKind::Body) else { return };
        if self.has_embedded_timing_control(sub_tree, body) {
            self.builder.unsup(
                loc,
                format!("subroutine '{name}' contains timing controls ('#', '@', 'wait') and cannot be inlined in RTL"),
            );
            return;
        }

        let caller_reach = ctx.effective_reach(&mut self.builder, loc);
        let reach_is_one = self.is_const_one(caller_reach);
        let owner_inst = sub_inst.unwrap_or(ctx.inst_id);
        let gen_consts = self.collect_scope_gen_consts(owner_inst, &sub_prefix);

        let mut sub_ctx = ProcCtx::new(
            &mut self.builder,
            owner_inst,
            sub_prefix.clone(),
            gen_consts,
            ctx.kind,
            ctx.active_clock,
            loc,
        );
        sub_ctx.path_cond = caller_reach;
        sub_ctx.def_gated = ctx.def_gated || !reach_is_one;
        sub_ctx.cur_syms = ctx.cur_syms.clone();
        sub_ctx.slot_writes = ctx.slot_writes.clone();
        sub_ctx.mem_writes_start = ctx.mem_writes_start.clone();

        let formals: Vec<SubroutineFormal> = extract_subroutine_formals(&self.trees[sub_tree], sub_node);
        if arg_nodes.len() > formals.len() {
            self.builder.err(
                loc,
                format!("subroutine '{name}' takes at most {} arguments, got {}", formals.len(), arg_nodes.len()),
            );
            return;
        }

        let mut copy_outs: Vec<(LocalId, RtlType, NodeId)> = Vec::new();
        for (i, formal) in formals.iter().enumerate() {
            let Some(fty) = ({
                let mut arena = ExprArena::new();
                let mut binder = self.make_proc_binder(&sub_ctx, &mut arena);
                let r = binder.resolve_subroutine_dtype(sub_tree, formal.dtype, formal.decl, sub_inst, &sub_prefix);
                self.flush_expr_diags(binder.diags);
                r
            }) else {
                return;
            };
            let flid = self.alloc_local_id();
            let init_ov = match formal.dir {
                PortDir::Input | PortDir::Inout => {
                    if let Some(&arg_n) = arg_nodes.get(i) {
                        let mut arena = ExprArena::new();
                        let mut binder = self.make_proc_binder(ctx, &mut arena);
                        let eid_opt = binder.bind_assigned(tree_idx, arg_n, &fty);
                        self.flush_expr_diags(binder.diags);
                        eid_opt
                            .and_then(|eid| self.lower_in_proc(&arena, eid, ctx))
                            .unwrap_or_else(|| make_oob_obj_val(&mut self.builder, &fty, loc))
                    } else if let Some(def_n) = formal.default_expr {
                        let mut arena = ExprArena::new();
                        let mut binder = self.make_proc_binder(&sub_ctx, &mut arena);
                        let eid_opt = binder.bind_assigned(sub_tree, def_n, &fty);
                        self.flush_expr_diags(binder.diags);
                        eid_opt
                            .and_then(|eid| self.lower_in_proc(&arena, eid, &sub_ctx))
                            .unwrap_or_else(|| make_oob_obj_val(&mut self.builder, &fty, loc))
                    } else {
                        self.builder.err(loc, format!("missing argument '{}' in call to '{name}'", formal.name));
                        make_oob_obj_val(&mut self.builder, &fty, loc)
                    }
                }
                PortDir::Output => make_oob_obj_val(&mut self.builder, &fty, loc),
                PortDir::Ref => {
                    self.builder
                        .unsup(loc, format!("'ref' argument '{}' on '{name}' is not supported in RTL", formal.name));
                    return;
                }
            };
            if matches!(formal.dir, PortDir::Output | PortDir::Inout) {
                if let Some(&arg_n) = arg_nodes.get(i) {
                    copy_outs.push((flid, fty.clone(), arg_n));
                }
            }
            let cval = init_ov.as_packed().and_then(|v| self.builder.const_cval(v).cloned());
            sub_ctx.cur_locals.insert(flid, init_ov);
            sub_ctx
                .local_decls
                .insert(formal.name.clone(), LocalBinding { id: flid, ty: fty, packed_bounds: None, cval });
        }

        // Optional function return local if calling a non-void function as a statement.
        if sub_kind == ElabItemKind::FunctionDecl {
            let Some(ret_ty) = ({
                let mut arena = ExprArena::new();
                let mut binder = self.make_proc_binder(&sub_ctx, &mut arena);
                let r = binder.resolve_function_return_type(sub_tree, sub_node, sub_inst, &sub_prefix);
                self.flush_expr_diags(binder.diags);
                r
            }) else {
                return;
            };
            if ret_ty != RtlType::Void {
                let rlid = self.alloc_local_id();
                let def_v = make_oob_obj_val(&mut self.builder, &ret_ty, loc);
                sub_ctx.cur_locals.insert(rlid, def_v);
                sub_ctx.ret_local = Some((rlid, ret_ty.clone()));
                sub_ctx
                    .local_decls
                    .insert(name.clone(), LocalBinding { id: rlid, ty: ret_ty, packed_bounds: None, cval: None });
            }
        }

        self.call_stack.push(callee);
        let items: Vec<NodeId> = self.trees[sub_tree]
            .children(body)
            .filter(|&c| self.trees[sub_tree].kind(c) != SyntaxKind::Attribute)
            .collect();
        for item_n in items {
            self.exec_stmt(sub_tree, item_n, &mut sub_ctx);
        }
        self.call_stack.pop();

        ctx.cur_syms = sub_ctx.cur_syms;
        ctx.slot_writes = sub_ctx.slot_writes;

        // Copy `output` and `inout` formals back into caller actuals.
        for (flid, fty, actual_lhs) in copy_outs {
            let Some(out_ov) = sub_ctx.cur_locals.get(&flid).cloned() else { continue };
            let _ = mux_obj_val;
            self.write_subroutine_output_to_caller(tree_idx, actual_lhs, out_ov, &fty, ctx, loc);
        }
    }

    fn write_subroutine_output_to_caller(
        &mut self,
        tree_idx: usize,
        lhs_node: NodeId,
        val: ObjVal,
        formal_ty: &RtlType,
        ctx: &mut ProcCtx,
        loc: Loc,
    ) {
        self.check_modport_lhs_legality(ctx.inst_id, tree_idx, lhs_node, &ctx.scope_prefix, loc);
        let mut arena = ExprArena::new();
        let Some(lhs_eid) = self.bind_in_proc(tree_idx, lhs_node, ctx, &mut arena, false) else { return };
        let lhs_ty = arena.get(lhs_eid).ty.clone();
        let coerced = self.coerce_obj_val(val, formal_ty, &lhs_ty, loc);
        self.exec_direct_bound_write(&arena, lhs_eid, coerced, ctx, loc);
    }

    fn exec_direct_bound_write(
        &mut self,
        arena: &ExprArena,
        lhs_eid: sv_semantic::ExprId,
        rhs_val: ObjVal,
        ctx: &mut ProcCtx,
        loc: Loc,
    ) {
        // Blocking copy-out per IEEE 1800 section 13.3: task/function `output` arguments assign by blocking copy on return.
        self.apply_bound_lhs_write(arena, lhs_eid, rhs_val, false, ctx, loc);
    }

    pub(crate) fn exec_initial_block(&mut self, inst_id: InstanceId, item: &ElabItem) {
        let Some(body) =
            self.trees[item.tree].children(item.node).find(|&c| self.trees[item.tree].kind(c) != SyntaxKind::Attribute)
        else {
            return;
        };
        if self.has_embedded_timing_control(item.tree, body) {
            self.builder
                .unsup(item.loc, "initial block with timing control ('#', '@', 'wait') is not synthesizable in RTL");
            return;
        }

        let mem_marks: Vec<usize> = self.builder.memories.iter().map(|m| m.writes.len()).collect();
        let mut ctx = ProcCtx::new(
            &mut self.builder,
            inst_id,
            item.scope_prefix.clone(),
            item.gen_consts.clone(),
            ProcKind::Clocked,
            None,
            item.loc,
        );
        ctx.in_initial = true;
        self.exec_stmt(item.tree, body, &mut ctx);

        // Extract constant `CVal`s written to slots into `self.slots[sidx].init`.
        let mut sidxs: Vec<usize> = ctx.slot_writes.keys().copied().collect();
        sidxs.sort_unstable();
        for sidx in sidxs {
            let sw = &ctx.slot_writes[&sidx];
            if let Some(cv) = self.builder.const_cval(sw.next_val).cloned() {
                let w = self.slots[sidx].width;
                let s = self.slots[sidx].signed;
                self.slots[sidx].init = Some(cv.cast(w, s));
            } else {
                // Partial slice initialization on a variable (`initial q[3:0] = 4'hA;`):
                // every written bit must fold to a constant.
                let w = self.slots[sidx].width;
                let s = self.slots[sidx].signed;
                let four = self.slots[sidx].four_state;
                let mut base = self.slots[sidx].init.clone().unwrap_or_else(|| {
                    if four {
                        CVal::all_x(w, s)
                    } else {
                        CVal::new(w, s, 0)
                    }
                });
                let mut any_written = false;
                let mut all_const = true;
                for b in 0..w {
                    if sw.ever_written[b as usize] {
                        any_written = true;
                        let bit_v = self.builder.alloc_op(
                            crate::ir::NetOp::Slice { hi: b, lo: b },
                            vec![sw.next_val],
                            1,
                            false,
                            four,
                            sw.first_loc,
                        );
                        if let Some(bcv) = self.builder.const_cval(bit_v).cloned() {
                            base = set_cval_bit(&base, b, bcv.bit_state(0));
                        } else {
                            all_const = false;
                            break;
                        }
                    }
                }
                if any_written && all_const {
                    self.slots[sidx].init = Some(base);
                } else {
                    self.builder.unsup(
                        sw.first_loc,
                        format!("non-constant initial assignment to '{}' is not supported", self.slots[sidx].name),
                    );
                }
            }
        }

        // Convert `initial` memory writes into `Memory::init` entries and remove them from runtime `Memory::writes`.
        for (mi, &mark) in mem_marks.iter().enumerate() {
            let tail: Vec<_> = self.builder.memories[mi].writes.drain(mark..).collect();
            let depth = self.builder.memories[mi].depth;
            let mw = self.builder.memories[mi].width;
            let ms = self.builder.memories[mi].signed;
            for wp in tail {
                let Some(en_cv) = self.builder.const_cval(wp.enable).cloned() else {
                    let mname = self.builder.memories[mi].name.clone();
                    self.builder.unsup(wp.loc, format!("non-constant initial write enable on memory '{mname}'"));
                    continue;
                };
                if !en_cv.known() || !en_cv.is_true() {
                    continue;
                }
                if let (Some(ac), Some(dc)) =
                    (self.builder.const_cval(wp.addr).cloned(), self.builder.const_cval(wp.data).cloned())
                {
                    if ac.known() && ac.i() >= 0 && (ac.i() as u32) < depth {
                        let widx = ac.i() as u32;
                        let casted = dc.cast(mw, ms);
                        let m = &mut self.builder.memories[mi];
                        m.init.retain(|(idx, _)| *idx != widx);
                        m.init.push((widx, casted));
                    }
                } else {
                    let mname = self.builder.memories[mi].name.clone();
                    self.builder.unsup(wp.loc, format!("non-constant initial write to memory '{mname}'"));
                }
            }
        }
    }
}

fn expr_subtree_has_side_effect(t: &Tree, root: NodeId) -> bool {
    for n in crate::lower::subtree_preorder(t, root) {
        match t.kind(n) {
            SyntaxKind::PrefixExpr | SyntaxKind::PostfixExpr => {
                if crate::proc::own_toks(t, n).iter().any(|op| matches!(*op, "++" | "--")) {
                    return true;
                }
            }
            SyntaxKind::BinaryExpr => {
                if let Some(op) = crate::proc::own_toks(t, n).into_iter().next() {
                    if crate::proc::is_assign_op(op) {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

fn split_for_header(t: &Tree, stmt: NodeId) -> (Vec<NodeId>, Option<NodeId>, Vec<NodeId>, Option<NodeId>) {
    let mut inits = Vec::new();
    let mut cond = None;
    let mut steps = Vec::new();
    let mut body = None;
    let mut semi_count = 0usize;
    let mut paren_depth = 0i32;
    let mut past_header = false;

    for ch in &t.node(stmt).children {
        match *ch {
            Child::Token(ti) => {
                let tok = &t.tokens[ti as usize];
                if tok.kind.is_trivia() {
                    continue;
                }
                match &*tok.text {
                    "(" => paren_depth += 1,
                    ")" => {
                        paren_depth -= 1;
                        if paren_depth <= 0 {
                            past_header = true;
                        }
                    }
                    ";" if !past_header => semi_count += 1,
                    _ => {}
                }
            }
            Child::Node(n) => {
                if matches!(t.kind(n), SyntaxKind::Attribute | SyntaxKind::StmtLabel) {
                    continue;
                }
                if past_header {
                    body = Some(n);
                } else if semi_count == 0 {
                    inits.push(n);
                } else if semi_count == 1 {
                    cond = Some(n);
                } else {
                    steps.push(n);
                }
            }
        }
    }
    (inits, cond, steps, body)
}

fn parse_foreach_header(t: &Tree, stmt: NodeId) -> Option<(String, Vec<Option<String>>)> {
    // `foreach` tokens directly on `stmt`: `foreach`, `(`, `<arr_path>`, `[`, `i`, `,`, `j`, `]`, `)`
    let mut in_paren = false;
    let mut in_bracket = false;
    let mut arr_parts: Vec<&str> = Vec::new();
    let mut vars: Vec<Option<String>> = Vec::new();
    let mut cur_var: Option<String> = None;

    for ch in &t.node(stmt).children {
        let Child::Token(ti) = *ch else { continue };
        let tok = &t.tokens[ti as usize];
        if tok.kind.is_trivia() {
            continue;
        }
        let txt = &*tok.text;
        if !in_paren {
            if txt == "(" {
                in_paren = true;
            }
            continue;
        }
        if !in_bracket {
            if txt == "[" {
                in_bracket = true;
            } else if txt == ")" {
                break;
            } else {
                arr_parts.push(txt);
            }
        } else if txt == "]" {
            if !vars.is_empty() || cur_var.is_some() {
                vars.push(cur_var.take());
            }
            break;
        } else if txt == "," {
            vars.push(cur_var.take());
        } else {
            let clean = txt.strip_prefix('\\').unwrap_or(txt).to_string();
            cur_var = Some(clean);
        }
    }
    if arr_parts.is_empty() {
        return None;
    }
    Some((arr_parts.concat(), vars))
}

fn collect_all_array_dims(ty: &RtlType, packed_1d: Option<(i128, i128)>, out: &mut Vec<(i128, i128)>) {
    match ty {
        RtlType::UnpackedArray { elem, dims } => {
            out.extend_from_slice(dims);
            collect_all_array_dims(elem, packed_1d, out);
        }
        RtlType::PackedArray { elem, left, right, .. } => {
            out.push((*left, *right));
            collect_all_array_dims(elem, None, out);
        }
        _ if ty.is_packed() => {
            if let Some(w) = ty.bit_width() {
                out.push(packed_1d.unwrap_or(((w as i128) - 1, 0)));
            }
        }
        _ => {}
    }
}

fn call_stmt_arg_nodes(t: &Tree, call_node: NodeId) -> Vec<NodeId> {
    let Some(al) = t.child(call_node, SyntaxKind::ArgList) else { return Vec::new() };
    let mut out = Vec::new();
    for c in t.children(al).filter(|&x| t.kind(x) != SyntaxKind::Attribute) {
        if t.kind(c) == SyntaxKind::Arg {
            if let Some(e) = t.children(c).find(|&x| t.kind(x) != SyntaxKind::Attribute) {
                out.push(e);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn set_cval_bit(cv: &CVal, bit: u32, bs: sv_semantic::BitState) -> CVal {
    let mut states = Vec::with_capacity(cv.w as usize);
    for i in 0..cv.w {
        states.push(if i == bit { bs } else { cv.bit_state(i) });
    }
    // Rebuild `CVal` from per-bit `BitState`s MSB-to-LSB!
    let mut acc: Option<CVal> = None;
    for &st in states.iter().rev() {
        let one_bit = match st {
            sv_semantic::BitState::Zero => CVal::new(1, false, 0),
            sv_semantic::BitState::One => CVal::new(1, false, 1),
            sv_semantic::BitState::X => CVal::all_x(1, false),
            sv_semantic::BitState::Z => CVal::all_z(1, false),
        };
        acc = Some(match acc {
            None => one_bit,
            Some(prev) => CVal::concat(&prev, &one_bit),
        });
    }
    acc.unwrap_or_else(|| CVal::new(1, false, 0)).cast(cv.w, cv.s)
}
