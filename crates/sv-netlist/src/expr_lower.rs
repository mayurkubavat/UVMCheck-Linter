//! Lowering of bound [`ExprArena`] trees into word-level [`NetBuilder`] nodes.

use sv_semantic::fx::FxMap;
use sv_semantic::{
    BinaryOp, CVal, ExprArena, ExprId, ExprKind, InsideItem, InstanceId, Loc, LocalId, RtlType, SymbolId, SysFunc,
    UnaryOp,
};
use sv_syntax::NodeId;

use crate::builder::{BuildKind, NetBuilder};
use crate::ir::{ClockSpec, Edge, LowerConfig, MemId, NetOp, SampledNode, ValueId};

/// Static storage shape allocated for an elaborated [`SymbolId`].
#[derive(Clone, Debug)]
pub(crate) enum ObjSlot {
    /// Packed scalar, vector, packed struct/union/array, or enum backed by `slots[slot_idx]`.
    Packed(usize),
    /// Constant parameter or genvar (`CVal`, `four_state`).
    Const(CVal, bool),
    /// Unpacked struct/union decomposed into named fields.
    Struct(Vec<(String, ObjSlot)>),
    /// Scalarized unpacked array (top-level aggregate port or multi-dimensional / struct-element array).
    Elems { left: i128, right: i128, elems: Vec<ObjSlot> },
    /// 1-D unpacked array of packed words lowered to [`MemId`].
    Memory(MemId),
}

/// Symbolic runtime value of an arbitrary [`RtlType`] inside procedural or continuous lowering.
#[derive(Clone, Debug)]
pub(crate) enum ObjVal {
    /// Packed value node.
    Packed(ValueId),
    /// Unpacked struct value.
    Struct(Vec<(String, ObjVal)>),
    /// Scalarized unpacked array elements (`elems[0]` corresponds to `min(left, right)`).
    Elems { left: i128, right: i128, elems: Vec<ObjVal> },
    /// Reference to a [`MemId`] array.
    Memory(MemId),
}

impl ObjVal {
    pub fn as_packed(&self) -> Option<ValueId> {
        match self {
            ObjVal::Packed(v) => Some(*v),
            _ => None,
        }
    }
}

/// Callback interface allowing [`ExprLowerer`] to inline user-defined `function` calls
/// inside procedural or continuous contexts.
pub(crate) trait FuncInliner {
    #[allow(clippy::too_many_arguments)]
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
    ) -> Option<ObjVal>;
}

/// Read-side lowering context for one expression tree.
pub(crate) struct ExprLowerer<'a, F: FuncInliner> {
    pub arena: &'a ExprArena,
    pub config: &'a LowerConfig,
    pub active_clock: Option<ClockSpec>,
    pub sym_slots: &'a FxMap<SymbolId, ObjSlot>,
    pub slot_placeholders: &'a [ValueId],
    pub cur_syms: &'a FxMap<SymbolId, ObjVal>,
    pub cur_locals: &'a FxMap<LocalId, ObjVal>,
    pub inliner: &'a mut F,
}

impl<'a, F: FuncInliner> ExprLowerer<'a, F> {
    pub fn lower_packed(&mut self, builder: &mut NetBuilder, id: ExprId) -> Option<ValueId> {
        self.lower_any(builder, id)?.as_packed()
    }

    pub fn lower_any(&mut self, builder: &mut NetBuilder, id: ExprId) -> Option<ObjVal> {
        let expr = self.arena.get(id);
        let loc = expr.loc;

        if !self.config.allow_four_state_ops {
            if let Some(uloc) = expr_has_isunknown(self.arena, id) {
                builder.unsup(uloc, "$isunknown requires allow_four_state_ops");
                return None;
            }
        }
        if let Some(cv) = &expr.cval {
            if expr.ty.is_packed() {
                let four = expr.ty.is_four_state();
                return Some(ObjVal::Packed(builder.alloc_const(cv.clone(), four, loc)));
            }
        }

        match &expr.kind {
            ExprKind::Symbol(sid) => {
                if let Some(ov) = self.cur_syms.get(sid) {
                    return Some(ov.clone());
                }
                let Some(slot) = self.sym_slots.get(sid) else {
                    builder.err(loc, format!("internal: missing signal slot for symbol {:?}", sid));
                    return None;
                };
                Some(self.read_slot_default(builder, slot, loc))
            }
            ExprKind::Local(lid) => {
                if let Some(ov) = self.cur_locals.get(lid) {
                    return Some(ov.clone());
                }
                let (w, s, four) = expr.ty.integral_shape().unwrap_or((1, false, true));
                let def = if four { CVal::all_x(w, s) } else { CVal::new(w, s, 0) };
                Some(ObjVal::Packed(builder.alloc_const(def, four, loc)))
            }
            ExprKind::Const(cv) => {
                let four = expr.ty.is_four_state();
                Some(ObjVal::Packed(builder.alloc_const(cv.clone(), four, loc)))
            }
            ExprKind::UnbasedUnsized(bs) => {
                let (w, s, four) = expr.ty.integral_shape().unwrap_or((1, false, true));
                let cv = match bs {
                    sv_semantic::BitState::Zero => CVal::new(w, s, 0),
                    sv_semantic::BitState::One => CVal::new(w, s, -1),
                    sv_semantic::BitState::X => CVal::all_x(w, s),
                    sv_semantic::BitState::Z => CVal::all_z(w, s),
                };
                Some(ObjVal::Packed(builder.alloc_const(cv, four, loc)))
            }
            ExprKind::Unary { op, arg } => {
                let av = self.lower_packed(builder, *arg)?;
                let (w, s, four) = expr.ty.integral_shape()?;
                let res = match op {
                    UnaryOp::Plus => av,
                    UnaryOp::Neg => builder.alloc_op(NetOp::Neg, vec![av], w, s, four, loc),
                    UnaryOp::BitNot => builder.alloc_op(NetOp::Not, vec![av], w, s, four, loc),
                    UnaryOp::LogNot => builder.alloc_not1(av, loc),
                    UnaryOp::RedAnd => builder.alloc_op(NetOp::RedAnd, vec![av], 1, false, four, loc),
                    UnaryOp::RedNand => {
                        let r = builder.alloc_op(NetOp::RedAnd, vec![av], 1, false, four, loc);
                        builder.alloc_op(NetOp::Not, vec![r], 1, false, four, loc)
                    }
                    UnaryOp::RedOr => builder.alloc_op(NetOp::RedOr, vec![av], 1, false, four, loc),
                    UnaryOp::RedNor => {
                        let r = builder.alloc_op(NetOp::RedOr, vec![av], 1, false, four, loc);
                        builder.alloc_op(NetOp::Not, vec![r], 1, false, four, loc)
                    }
                    UnaryOp::RedXor => builder.alloc_op(NetOp::RedXor, vec![av], 1, false, four, loc),
                    UnaryOp::RedXnor => {
                        let r = builder.alloc_op(NetOp::RedXor, vec![av], 1, false, four, loc);
                        builder.alloc_op(NetOp::Not, vec![r], 1, false, four, loc)
                    }
                };
                Some(ObjVal::Packed(res))
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let lv = self.lower_packed(builder, *lhs)?;
                let rv = self.lower_packed(builder, *rhs)?;
                let (w, s, four) = expr.ty.integral_shape()?;
                let lhs_signed = self.arena.get(*lhs).ty.is_signed();
                let res = match op {
                    BinaryOp::Add => builder.alloc_op(NetOp::Add, vec![lv, rv], w, s, four, loc),
                    BinaryOp::Sub => builder.alloc_op(NetOp::Sub, vec![lv, rv], w, s, four, loc),
                    BinaryOp::Mul => builder.alloc_op(NetOp::Mul, vec![lv, rv], w, s, four, loc),
                    BinaryOp::Div => {
                        let nop = if s { NetOp::SDiv } else { NetOp::UDiv };
                        builder.alloc_op(nop, vec![lv, rv], w, s, four, loc)
                    }
                    BinaryOp::Mod => {
                        let nop = if s { NetOp::SMod } else { NetOp::UMod };
                        builder.alloc_op(nop, vec![lv, rv], w, s, four, loc)
                    }
                    BinaryOp::Pow => builder.alloc_op(NetOp::Pow, vec![lv, rv], w, s, four, loc),
                    BinaryOp::BitAnd => builder.alloc_op(NetOp::And, vec![lv, rv], w, s, four, loc),
                    BinaryOp::BitOr => builder.alloc_op(NetOp::Or, vec![lv, rv], w, s, four, loc),
                    BinaryOp::BitXor => builder.alloc_op(NetOp::Xor, vec![lv, rv], w, s, four, loc),
                    BinaryOp::BitXnor => builder.alloc_op(NetOp::Xnor, vec![lv, rv], w, s, four, loc),
                    BinaryOp::LogAnd => builder.alloc_and1(lv, rv, loc),
                    BinaryOp::LogOr => builder.alloc_or1(lv, rv, loc),
                    BinaryOp::LogImp => {
                        let nl = builder.alloc_not1(lv, loc);
                        builder.alloc_or1(nl, rv, loc)
                    }
                    BinaryOp::LogEqv => {
                        let bl = builder.alloc_bool(lv, loc);
                        let br = builder.alloc_bool(rv, loc);
                        builder.alloc_op(NetOp::Xnor, vec![bl, br], 1, false, four, loc)
                    }
                    BinaryOp::Shl => builder.alloc_op(NetOp::Shl, vec![lv, rv], w, s, four, loc),
                    BinaryOp::LShr => builder.alloc_op(NetOp::LShr, vec![lv, rv], w, s, four, loc),
                    BinaryOp::AShr => {
                        // IEEE 1800 section 11.4.10: `>>>` sign-fills only when LHS is signed.
                        let nop = if lhs_signed { NetOp::AShr } else { NetOp::LShr };
                        builder.alloc_op(nop, vec![lv, rv], w, s, four, loc)
                    }
                    BinaryOp::Eq => builder.alloc_op(NetOp::Eq, vec![lv, rv], 1, false, four, loc),
                    BinaryOp::Neq => builder.alloc_op(NetOp::Neq, vec![lv, rv], 1, false, four, loc),
                    BinaryOp::CaseEq => builder.alloc_op(NetOp::CaseEq, vec![lv, rv], 1, false, false, loc),
                    BinaryOp::CaseNeq => builder.alloc_op(NetOp::CaseNeq, vec![lv, rv], 1, false, false, loc),
                    BinaryOp::WildEq | BinaryOp::WildNeq => {
                        let Some(pat) = self.arena.get(*rhs).cval.clone() else {
                            builder
                                .err(loc, "wildcard equality ('==?' / '!=?') requires a constant right-hand pattern");
                            return None;
                        };
                        let nop = if *op == BinaryOp::WildEq {
                            NetOp::WildEq { rhs: pat }
                        } else {
                            NetOp::WildNeq { rhs: pat }
                        };
                        builder.alloc_op(nop, vec![lv], 1, false, four, loc)
                    }
                    BinaryOp::Lt => {
                        let nop = if lhs_signed { NetOp::SLt } else { NetOp::ULt };
                        builder.alloc_op(nop, vec![lv, rv], 1, false, four, loc)
                    }
                    BinaryOp::Le => {
                        let nop = if lhs_signed { NetOp::SLe } else { NetOp::ULe };
                        builder.alloc_op(nop, vec![lv, rv], 1, false, four, loc)
                    }
                    BinaryOp::Gt => {
                        let nop = if lhs_signed { NetOp::SLt } else { NetOp::ULt };
                        builder.alloc_op(nop, vec![rv, lv], 1, false, four, loc)
                    }
                    BinaryOp::Ge => {
                        let nop = if lhs_signed { NetOp::SLe } else { NetOp::ULe };
                        builder.alloc_op(nop, vec![rv, lv], 1, false, four, loc)
                    }
                };
                Some(ObjVal::Packed(res))
            }
            ExprKind::Cond { cond, then_val, else_val } => {
                let raw_c = self.lower_packed(builder, *cond)?;
                let cv = builder.alloc_bool(raw_c, loc);
                let tv = self.lower_any(builder, *then_val)?;
                let ev = self.lower_any(builder, *else_val)?;
                Some(mux_obj_val(builder, cv, tv, ev, &expr.ty, loc))
            }
            ExprKind::Inside { expr: target, items } => {
                let tv = self.lower_packed(builder, *target)?;
                let target_signed = self.arena.get(*target).ty.is_signed();
                let mut acc: Option<ValueId> = None;
                for item in items {
                    let hit = match item {
                        InsideItem::Single(ie) => {
                            let is_s = self.arena.get(*ie).ty.is_signed();
                            if let Some(pat) = self.arena.get(*ie).cval.clone() {
                                if !pat.known() {
                                    builder.alloc_op(NetOp::WildEq { rhs: pat }, vec![tv], 1, false, true, loc)
                                } else {
                                    let iv = self.lower_packed(builder, *ie)?;
                                    let (tw, _, tf) = builder.node_shape(tv);
                                    let (iw, _, if4) = builder.node_shape(iv);
                                    let cw = tw.max(iw);
                                    let cs = target_signed && is_s;
                                    let lt = builder.resize_val(tv, cw, cs, cs, loc);
                                    let ri = builder.resize_val(iv, cw, cs, cs, loc);
                                    builder.alloc_op(NetOp::Eq, vec![lt, ri], 1, false, tf || if4, loc)
                                }
                            } else {
                                let iv = self.lower_packed(builder, *ie)?;
                                let (tw, _, tf) = builder.node_shape(tv);
                                let (iw, _, if4) = builder.node_shape(iv);
                                let cw = tw.max(iw);
                                let cs = target_signed && is_s;
                                let lt = builder.resize_val(tv, cw, cs, cs, loc);
                                let ri = builder.resize_val(iv, cw, cs, cs, loc);
                                builder.alloc_op(NetOp::Eq, vec![lt, ri], 1, false, tf || if4, loc)
                            }
                        }
                        InsideItem::Range(lo_e, hi_e) => {
                            let ls = self.arena.get(*lo_e).ty.is_signed();
                            let hs = self.arena.get(*hi_e).ty.is_signed();
                            let lov = self.lower_packed(builder, *lo_e)?;
                            let hiv = self.lower_packed(builder, *hi_e)?;
                            let (tw, _, tf) = builder.node_shape(tv);
                            let (lw, _, lf) = builder.node_shape(lov);
                            let (hw, _, hf) = builder.node_shape(hiv);
                            let cw = tw.max(lw).max(hw);
                            let cs = target_signed && ls && hs;
                            let f4 = tf || lf || hf;
                            let lt = builder.resize_val(tv, cw, cs, cs, loc);
                            let llo = builder.resize_val(lov, cw, cs, cs, loc);
                            let lhi = builder.resize_val(hiv, cw, cs, cs, loc);
                            let cmp_op = if cs { NetOp::SLe } else { NetOp::ULe };
                            let ge_lo = builder.alloc_op(cmp_op.clone(), vec![llo, lt], 1, false, f4, loc);
                            let le_hi = builder.alloc_op(cmp_op, vec![lt, lhi], 1, false, f4, loc);
                            builder.alloc_and1(ge_lo, le_hi, loc)
                        }
                    };
                    acc = Some(match acc {
                        None => hit,
                        Some(prev) => builder.alloc_or1(prev, hit, loc),
                    });
                }
                let out = acc.unwrap_or_else(|| builder.alloc_const(CVal::new(1, false, 0), false, loc));
                Some(ObjVal::Packed(out))
            }
            ExprKind::Concat(parts) => {
                let (w, s, four) = expr.ty.integral_shape()?;
                let mut vals = Vec::with_capacity(parts.len());
                for &p in parts {
                    if matches!(self.arena.get(p).kind, ExprKind::Replicate { count: 0, .. }) {
                        continue;
                    }
                    vals.push(self.lower_packed(builder, p)?);
                }
                if vals.is_empty() {
                    builder.unsup(loc, "zero-width concatenation is not supported");
                    return None;
                }
                Some(ObjVal::Packed(builder.alloc_op(NetOp::Concat, vals, w, s, four, loc)))
            }
            ExprKind::Replicate { count, expr: inner } => {
                let (w, s, four) = expr.ty.integral_shape()?;
                if *count == 0 {
                    return Some(ObjVal::Packed(builder.alloc_const(CVal::new(1, false, 0), false, loc)));
                }
                let iv = self.lower_packed(builder, *inner)?;
                let vals = vec![iv; *count as usize];
                Some(ObjVal::Packed(builder.alloc_op(NetOp::Concat, vals, w, s, four, loc)))
            }
            ExprKind::Member { base, field, packed_slice } => {
                let bv = self.lower_any(builder, *base)?;
                if let Some((lo, slice_w)) = *packed_slice {
                    let pv = bv.as_packed()?;
                    let (_, s, four) = expr.ty.integral_shape()?;
                    let out =
                        builder.alloc_op(NetOp::Slice { hi: lo + slice_w - 1, lo }, vec![pv], slice_w, s, four, loc);
                    return Some(ObjVal::Packed(out));
                }
                match bv {
                    ObjVal::Struct(fields) => fields.into_iter().find(|(n, _)| n == field).map(|(_, v)| v),
                    _ => {
                        builder.err(loc, format!("cannot access unpacked member '{field}' on non-struct"));
                        None
                    }
                }
            }
            ExprKind::BitSelect { base, index, bounds, elem_width } => {
                let bv = self.lower_packed(builder, *base)?;
                let (total_w, _, _) = builder.node_shape(bv);
                let (out_w, out_s, out_four) = expr.ty.integral_shape()?;
                if let Some(icv) = self.arena.get(*index).cval.clone() {
                    if !icv.known() {
                        let oob = if out_four { CVal::all_x(out_w, out_s) } else { CVal::new(out_w, out_s, 0) };
                        return Some(ObjVal::Packed(builder.alloc_const(oob, out_four, loc)));
                    }
                    if let Some(bit_lo) = sv_semantic::expr::map_bit_offset(icv.i(), *bounds, *elem_width, total_w) {
                        let out = builder.alloc_op(
                            NetOp::Slice { hi: bit_lo + *elem_width - 1, lo: bit_lo },
                            vec![bv],
                            out_w,
                            out_s,
                            out_four,
                            loc,
                        );
                        return Some(ObjVal::Packed(out));
                    }
                    let oob = if out_four { CVal::all_x(out_w, out_s) } else { CVal::new(out_w, out_s, 0) };
                    return Some(ObjVal::Packed(builder.alloc_const(oob, out_four, loc)));
                }
                let iv = self.lower_packed(builder, *index)?;
                let idx_signed = self.arena.get(*index).ty.is_signed();
                let out = lower_dynamic_packed_slice(
                    builder,
                    bv,
                    iv,
                    idx_signed,
                    *bounds,
                    *elem_width,
                    1,
                    true,
                    out_s,
                    out_four,
                    loc,
                );
                Some(ObjVal::Packed(out))
            }
            ExprKind::PartSelect { base, msb, lsb, bounds, elem_width } => {
                let bv = self.lower_packed(builder, *base)?;
                let (total_w, _, _) = builder.node_shape(bv);
                let (out_w, out_s, out_four) = expr.ty.integral_shape()?;
                let count = ((*msb - *lsb).unsigned_abs() as usize) + 1;
                let step = if bounds.0 >= bounds.1 { 1i128 } else { -1i128 };
                let low_idx = if bounds.0 >= bounds.1 { (*msb).min(*lsb) } else { (*msb).max(*lsb) };

                // Fast path when the full range is an in-bounds contiguous slice.
                let first_off = sv_semantic::expr::map_bit_offset(low_idx, *bounds, *elem_width, total_w);
                let last_idx = low_idx + ((count.saturating_sub(1)) as i128) * step;
                let last_off = sv_semantic::expr::map_bit_offset(last_idx, *bounds, *elem_width, total_w);
                if let (Some(lo_bit), Some(hi_elem_bit)) = (first_off, last_off) {
                    if lo_bit <= hi_elem_bit && (hi_elem_bit + *elem_width - lo_bit) == out_w {
                        let out = builder.alloc_op(
                            NetOp::Slice { hi: lo_bit + out_w - 1, lo: lo_bit },
                            vec![bv],
                            out_w,
                            out_s,
                            out_four,
                            loc,
                        );
                        return Some(ObjVal::Packed(out));
                    }
                }

                // Partial OOB part-select assembled element-by-element MSB-first.
                let mut msb_first = Vec::with_capacity(count);
                for k in (0..count).rev() {
                    let cur_idx = low_idx + (k as i128) * step;
                    let part = match sv_semantic::expr::map_bit_offset(cur_idx, *bounds, *elem_width, total_w) {
                        Some(off) => builder.alloc_op(
                            NetOp::Slice { hi: off + *elem_width - 1, lo: off },
                            vec![bv],
                            *elem_width,
                            false,
                            out_four,
                            loc,
                        ),
                        None => {
                            let oob = if out_four {
                                CVal::all_x(*elem_width, false)
                            } else {
                                CVal::new(*elem_width, false, 0)
                            };
                            builder.alloc_const(oob, out_four, loc)
                        }
                    };
                    msb_first.push(part);
                }
                Some(ObjVal::Packed(builder.alloc_op(NetOp::Concat, msb_first, out_w, out_s, out_four, loc)))
            }
            ExprKind::IndexedPartSelect { base, base_idx, width: count, ascending, bounds, elem_width } => {
                let bv = self.lower_packed(builder, *base)?;
                let (_out_w, out_s, out_four) = expr.ty.integral_shape()?;
                let iv = self.lower_packed(builder, *base_idx)?;
                let idx_signed = self.arena.get(*base_idx).ty.is_signed();
                let out = lower_dynamic_packed_slice(
                    builder,
                    bv,
                    iv,
                    idx_signed,
                    *bounds,
                    *elem_width,
                    *count,
                    *ascending,
                    out_s,
                    out_four,
                    loc,
                );
                Some(ObjVal::Packed(out))
            }
            ExprKind::ArrayIndex { base, index, dim } => {
                let base_val = self.lower_any(builder, *base)?;
                let idx_val = self.lower_packed(builder, *index)?;
                let idx_signed = self.arena.get(*index).ty.is_signed();
                self.lower_array_read(builder, base_val, idx_val, idx_signed, *dim, &expr.ty, loc)
            }
            ExprKind::Resize { arg, width, signed: ext_signed } => {
                let av = self.lower_packed(builder, *arg)?;
                let out_s = expr.ty.is_signed();
                Some(ObjVal::Packed(builder.resize_val(av, *width, *ext_signed, out_s, loc)))
            }
            ExprKind::CastSign { arg, signed: target_s } => {
                let av = self.lower_packed(builder, *arg)?;
                let (w, cur_s, four) = builder.node_shape(av);
                if cur_s == *target_s {
                    return Some(ObjVal::Packed(av));
                }
                let out = builder.alloc_op(NetOp::Slice { hi: w - 1, lo: 0 }, vec![av], w, *target_s, four, loc);
                Some(ObjVal::Packed(out))
            }
            ExprKind::CastFourState { arg, four_state } => {
                let av = self.lower_packed(builder, *arg)?;
                let (w, s, cur_four) = builder.node_shape(av);
                if cur_four == *four_state {
                    return Some(ObjVal::Packed(av));
                }
                Some(ObjVal::Packed(builder.alloc_op(
                    NetOp::CastFourState { four_state: *four_state },
                    vec![av],
                    w,
                    s,
                    *four_state,
                    loc,
                )))
            }
            ExprKind::SysCall { func, args } => {
                let a0 = self.lower_packed(builder, *args.first()?)?;
                match func {
                    SysFunc::CountOnes => {
                        Some(ObjVal::Packed(builder.alloc_op(NetOp::CountOnes, vec![a0], 32, true, false, loc)))
                    }
                    SysFunc::OneHot => {
                        Some(ObjVal::Packed(builder.alloc_op(NetOp::OneHot, vec![a0], 1, false, false, loc)))
                    }
                    SysFunc::OneHot0 => {
                        Some(ObjVal::Packed(builder.alloc_op(NetOp::OneHot0, vec![a0], 1, false, false, loc)))
                    }
                    SysFunc::IsUnknown => {
                        if !self.config.allow_four_state_ops {
                            builder.unsup(loc, "$isunknown requires allow_four_state_ops");
                            return None;
                        }
                        Some(ObjVal::Packed(builder.alloc_op(NetOp::IsUnknown, vec![a0], 1, false, false, loc)))
                    }
                    SysFunc::Clog2 => Some(ObjVal::Packed(lower_dynamic_clog2(builder, a0, loc))),
                    SysFunc::Signed | SysFunc::Unsigned => {
                        let (w, cur_s, four) = builder.node_shape(a0);
                        let target_s = expr.ty.is_signed();
                        if cur_s == target_s {
                            Some(ObjVal::Packed(a0))
                        } else {
                            Some(ObjVal::Packed(builder.alloc_op(
                                NetOp::Slice { hi: w - 1, lo: 0 },
                                vec![a0],
                                w,
                                target_s,
                                four,
                                loc,
                            )))
                        }
                    }
                    _ => {
                        builder.unsup(loc, format!("non-constant system function '{func:?}'"));
                        None
                    }
                }
            }
            ExprKind::Sampled { kind, expr: inner, depth, gate, clock, clock_posedge } => {
                let ev = self.lower_packed(builder, *inner)?;
                let gv = match gate {
                    Some(g) => {
                        let rg = self.lower_packed(builder, *g)?;
                        Some(builder.alloc_bool(rg, loc))
                    }
                    None => None,
                };
                let resolved_clk = match clock {
                    Some(c) => {
                        let rc = self.lower_packed(builder, *c)?;
                        let csig = extract_lsb_bit0(builder, rc, loc);
                        let edge = if *clock_posedge == Some(false) { Edge::Negedge } else { Edge::Posedge };
                        Some(ClockSpec { signal: csig, edge })
                    }
                    None => self.active_clock,
                };
                let (w, s, four) = expr.ty.integral_shape()?;
                let sidx = builder.sampled.len();
                let out = builder.alloc_raw(w, s, four, BuildKind::Sampled(sidx), None, loc);
                builder.sampled.push(SampledNode {
                    kind: *kind,
                    expr: ev,
                    depth: *depth,
                    clock: resolved_clk,
                    gate: gv,
                    out,
                    loc,
                });
                Some(ObjVal::Packed(out))
            }
            ExprKind::FuncCall { callee, callee_inst, callee_scope_prefix, name, args } => {
                let mut lowered_args = Vec::with_capacity(args.len());
                for &a in args {
                    lowered_args.push(self.lower_any(builder, a)?);
                }
                self.inliner.inline_func_call(
                    builder,
                    *callee,
                    *callee_inst,
                    callee_scope_prefix,
                    name,
                    &lowered_args,
                    &expr.ty,
                    loc,
                )
            }
            ExprKind::AssignPattern(entries) => match &expr.ty {
                RtlType::UnpackedStruct { fields, .. } => {
                    let mut out = Vec::with_capacity(fields.len());
                    for (idx, (fname, _)) in fields.iter().enumerate() {
                        let e_id = entries
                            .iter()
                            .find(|(k, _)| k.as_deref() == Some(fname.as_str()))
                            .or_else(|| entries.get(idx))
                            .map(|(_, e)| *e)?;
                        out.push((fname.clone(), self.lower_any(builder, e_id)?));
                    }
                    Some(ObjVal::Struct(out))
                }
                RtlType::UnpackedArray { dims, .. } if !dims.is_empty() => {
                    let (left, right) = dims[0];
                    let mut elems = Vec::with_capacity(entries.len());
                    // Note: in SV `logic [7:0] a [0:3] = '{d0, d1, d2, d3}`, positional pattern entries
                    // follow `left -> right`; normalize storage so `elems[0]` corresponds to `min(left, right)`.
                    for (_, e_id) in entries {
                        elems.push(self.lower_any(builder, *e_id)?);
                    }
                    if left > right {
                        elems.reverse();
                    }
                    Some(ObjVal::Elems { left, right, elems })
                }
                _ => {
                    builder.unsup(loc, "unsupported assignment pattern in expression");
                    None
                }
            },
        }
    }

    pub fn read_slot_default(&self, builder: &mut NetBuilder, slot: &ObjSlot, loc: Loc) -> ObjVal {
        match slot {
            ObjSlot::Packed(sidx) => ObjVal::Packed(self.slot_placeholders[*sidx]),
            ObjSlot::Const(cv, four) => ObjVal::Packed(builder.alloc_const(cv.clone(), *four, loc)),
            ObjSlot::Struct(fields) => ObjVal::Struct(
                fields.iter().map(|(n, fs)| (n.clone(), self.read_slot_default(builder, fs, loc))).collect(),
            ),
            ObjSlot::Elems { left, right, elems } => ObjVal::Elems {
                left: *left,
                right: *right,
                elems: elems.iter().map(|es| self.read_slot_default(builder, es, loc)).collect(),
            },
            ObjSlot::Memory(mid) => ObjVal::Memory(*mid),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_array_read(
        &self,
        builder: &mut NetBuilder,
        base_val: ObjVal,
        idx_val: ValueId,
        idx_signed: bool,
        dim: (i128, i128),
        elem_ty: &RtlType,
        loc: Loc,
    ) -> Option<ObjVal> {
        match base_val {
            ObjVal::Memory(mid) => {
                let (w, s, four) = elem_ty.integral_shape()?;
                let min_b = dim.0.min(dim.1);
                let max_b = dim.0.max(dim.1);
                let depth = ((max_b - min_b) as u32) + 1;
                let addr_w = addr_bits(depth);

                if let Some(icv) = builder.const_cval(idx_val).cloned() {
                    if !icv.known() || icv.i() < min_b || icv.i() > max_b {
                        let oob = if four { CVal::all_x(w, s) } else { CVal::new(w, s, 0) };
                        return Some(ObjVal::Packed(builder.alloc_const(oob, four, loc)));
                    }
                    let zero_off = icv.i() - min_b;
                    let addr = builder.alloc_const(CVal::new(addr_w, false, zero_off), false, loc);
                    let rd = builder.alloc_raw(w, s, four, BuildKind::MemRead { mem: mid, addr }, None, loc);
                    return Some(ObjVal::Packed(rd));
                }

                let (in_bounds, norm_addr) =
                    compute_unpacked_index_bounds(builder, idx_val, idx_signed, min_b, max_b, addr_w, loc);
                let rd = builder.alloc_raw(w, s, four, BuildKind::MemRead { mem: mid, addr: norm_addr }, None, loc);
                let oob_cv = if four { CVal::all_x(w, s) } else { CVal::new(w, s, 0) };
                let oob = builder.alloc_const(oob_cv, four, loc);
                let muxed = builder.alloc_op(NetOp::Mux, vec![in_bounds, rd, oob], w, s, four, loc);
                Some(ObjVal::Packed(muxed))
            }
            ObjVal::Elems { left, right, elems } => {
                let min_b = left.min(right);
                let max_b = left.max(right);
                if let Some(icv) = builder.const_cval(idx_val).cloned() {
                    if !icv.known() || icv.i() < min_b || icv.i() > max_b {
                        return Some(make_oob_obj_val(builder, elem_ty, loc));
                    }
                    let off = (icv.i() - min_b) as usize;
                    return elems.get(off).cloned();
                }
                let addr_w = addr_bits(elems.len() as u32);
                let (in_bounds, norm_addr) =
                    compute_unpacked_index_bounds(builder, idx_val, idx_signed, min_b, max_b, addr_w, loc);
                let (_, _, addr_four) = builder.node_shape(norm_addr);
                let mut acc = make_oob_obj_val(builder, elem_ty, loc);
                for (k, el) in elems.into_iter().enumerate().rev() {
                    let kc = builder.alloc_const(CVal::new(addr_w, false, k as i128), false, loc);
                    let eq = builder.alloc_op(NetOp::Eq, vec![norm_addr, kc], 1, false, addr_four, loc);
                    let hit = builder.alloc_and1(in_bounds, eq, loc);
                    acc = mux_obj_val(builder, hit, el, acc, elem_ty, loc);
                }
                Some(acc)
            }
            _ => {
                builder.err(loc, "cannot index non-array value");
                None
            }
        }
    }
}

/// Returns the source location of the first `SysFunc::IsUnknown` call reachable from `id`.
pub(crate) fn expr_has_isunknown(arena: &ExprArena, id: ExprId) -> Option<Loc> {
    let e = arena.get(id);
    match &e.kind {
        ExprKind::SysCall { func: SysFunc::IsUnknown, .. } => Some(e.loc),
        ExprKind::SysCall { args, .. } | ExprKind::FuncCall { args, .. } | ExprKind::Concat(args) => {
            args.iter().find_map(|&a| expr_has_isunknown(arena, a))
        }
        ExprKind::Unary { arg, .. }
        | ExprKind::Replicate { expr: arg, .. }
        | ExprKind::Member { base: arg, .. }
        | ExprKind::PartSelect { base: arg, .. }
        | ExprKind::Resize { arg, .. }
        | ExprKind::CastSign { arg, .. }
        | ExprKind::CastFourState { arg, .. } => expr_has_isunknown(arena, *arg),
        ExprKind::Binary { lhs, rhs, .. }
        | ExprKind::BitSelect { base: lhs, index: rhs, .. }
        | ExprKind::IndexedPartSelect { base: lhs, base_idx: rhs, .. }
        | ExprKind::ArrayIndex { base: lhs, index: rhs, .. } => {
            expr_has_isunknown(arena, *lhs).or_else(|| expr_has_isunknown(arena, *rhs))
        }
        ExprKind::Cond { cond, then_val, else_val } => expr_has_isunknown(arena, *cond)
            .or_else(|| expr_has_isunknown(arena, *then_val))
            .or_else(|| expr_has_isunknown(arena, *else_val)),
        ExprKind::Inside { expr: target, items } => {
            if let Some(loc) = expr_has_isunknown(arena, *target) {
                return Some(loc);
            }
            for item in items {
                match item {
                    InsideItem::Single(ie) => {
                        if let Some(loc) = expr_has_isunknown(arena, *ie) {
                            return Some(loc);
                        }
                    }
                    InsideItem::Range(lo, hi) => {
                        if let Some(loc) = expr_has_isunknown(arena, *lo).or_else(|| expr_has_isunknown(arena, *hi)) {
                            return Some(loc);
                        }
                    }
                }
            }
            None
        }
        ExprKind::Sampled { expr: inner, gate, clock, .. } => expr_has_isunknown(arena, *inner)
            .or_else(|| gate.and_then(|g| expr_has_isunknown(arena, g)))
            .or_else(|| clock.and_then(|c| expr_has_isunknown(arena, c))),
        ExprKind::AssignPattern(entries) => entries.iter().find_map(|(_, ae)| expr_has_isunknown(arena, *ae)),
        ExprKind::Symbol(_) | ExprKind::Local(_) | ExprKind::Const(_) | ExprKind::UnbasedUnsized(_) => None,
    }
}

/// Extracts physical bit 0 (`Slice { hi: 0, lo: 0 }`) as a 1-bit unsigned value per IEEE 1800 section 9.4.2.
pub(crate) fn extract_lsb_bit0(builder: &mut NetBuilder, val: ValueId, loc: Loc) -> ValueId {
    let (w, s, four) = builder.node_shape(val);
    if w <= 1 && !s {
        val
    } else {
        builder.alloc_op(NetOp::Slice { hi: 0, lo: 0 }, vec![val], 1, false, four, loc)
    }
}

/// Coerces an enable condition to a 2-state 1-bit `ValueId` (`CaseEq(b, 1'b1)` when 4-state) so `X`/`Z` never enables a write.
pub(crate) fn coerce_two_state_en(builder: &mut NetBuilder, cond: ValueId, loc: Loc) -> ValueId {
    let b = builder.alloc_bool(cond, loc);
    if builder.node_shape(b).2 {
        let one = builder.alloc_const(CVal::new(1, false, 1), false, loc);
        builder.alloc_op(NetOp::CaseEq, vec![b, one], 1, false, false, loc)
    } else {
        b
    }
}

pub(crate) fn addr_bits(depth: u32) -> u32 {
    if depth <= 1 {
        1
    } else {
        32 - (depth - 1).leading_zeros()
    }
}

/// Computes `(in_bounds: ValueId, zero_based_addr: ValueId)` for an unpacked array index `idx_val`
/// against closed bounds `[min_b ..= max_b]`.
pub(crate) fn compute_unpacked_index_bounds(
    builder: &mut NetBuilder,
    idx_val: ValueId,
    idx_signed: bool,
    min_b: i128,
    max_b: i128,
    addr_w: u32,
    loc: Loc,
) -> (ValueId, ValueId) {
    let (iw, _, if4) = builder.node_shape(idx_val);
    let calc_w = iw.max(32) + 2;
    let ext_idx = builder.resize_val(idx_val, calc_w, idx_signed, true, loc);
    let min_c = builder.alloc_const(CVal::new(calc_w, true, min_b), false, loc);
    let max_c = builder.alloc_const(CVal::new(calc_w, true, max_b), false, loc);
    let ge_min = builder.alloc_op(NetOp::SLe, vec![min_c, ext_idx], 1, false, if4, loc);
    let le_max = builder.alloc_op(NetOp::SLe, vec![ext_idx, max_c], 1, false, if4, loc);
    let in_bounds = builder.alloc_and1(ge_min, le_max, loc);
    let diff = builder.alloc_op(NetOp::Sub, vec![ext_idx, min_c], calc_w, false, if4, loc);
    let norm_addr = builder.resize_val(diff, addr_w.max(1), false, false, loc);
    (in_bounds, norm_addr)
}

/// Computes `(in_bounds: ValueId, bit_lo: ValueId)` for a dynamic bit-select or `+: / -:` part-select
/// on a packed vector with `bounds = (left, right)`, `elem_w`, `count` elements, and `total_w` bits.
#[allow(clippy::too_many_arguments)]
pub(crate) fn compute_packed_slice_offset(
    builder: &mut NetBuilder,
    idx_val: ValueId,
    idx_signed: bool,
    bounds: (i128, i128),
    elem_w: u32,
    count: u32,
    ascending_op: bool,
    total_w: u32,
    loc: Loc,
) -> (ValueId, ValueId) {
    let (iw, _, if4) = builder.node_shape(idx_val);
    let calc_w = iw.max(32).max(addr_bits(total_w)) + 2;
    let ext_idx = builder.resize_val(idx_val, calc_w, idx_signed, true, loc);

    let span = (count.saturating_sub(1)) as i128;
    let span_c = builder.alloc_const(CVal::new(calc_w, true, span), false, loc);

    let (lo_sv, hi_sv) = if count <= 1 {
        (ext_idx, ext_idx)
    } else if ascending_op {
        let hi = builder.alloc_op(NetOp::Add, vec![ext_idx, span_c], calc_w, true, if4, loc);
        (ext_idx, hi)
    } else {
        let lo = builder.alloc_op(NetOp::Sub, vec![ext_idx, span_c], calc_w, true, if4, loc);
        (lo, ext_idx)
    };

    let min_b = bounds.0.min(bounds.1);
    let max_b = bounds.0.max(bounds.1);
    let min_c = builder.alloc_const(CVal::new(calc_w, true, min_b), false, loc);
    let max_c = builder.alloc_const(CVal::new(calc_w, true, max_b), false, loc);
    let ge_min = builder.alloc_op(NetOp::SLe, vec![min_c, lo_sv], 1, false, if4, loc);
    let le_max = builder.alloc_op(NetOp::SLe, vec![hi_sv, max_c], 1, false, if4, loc);
    let in_bounds = builder.alloc_and1(ge_min, le_max, loc);

    let right_c = builder.alloc_const(CVal::new(calc_w, true, bounds.1), false, loc);
    let elem_off = if bounds.0 >= bounds.1 {
        // Descending `[left:right]`: `lo_sv - right`.
        builder.alloc_op(NetOp::Sub, vec![lo_sv, right_c], calc_w, false, if4, loc)
    } else {
        // Ascending `[left:right]` (e.g. `[0:7]`): `right - hi_sv`.
        builder.alloc_op(NetOp::Sub, vec![right_c, hi_sv], calc_w, false, if4, loc)
    };

    let bit_lo = if elem_w == 1 {
        elem_off
    } else {
        let ew_c = builder.alloc_const(CVal::new(calc_w, false, elem_w as i128), false, loc);
        builder.alloc_op(NetOp::Mul, vec![elem_off, ew_c], calc_w, false, if4, loc)
    };
    (in_bounds, bit_lo)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_dynamic_packed_slice(
    builder: &mut NetBuilder,
    base_val: ValueId,
    idx_val: ValueId,
    idx_signed: bool,
    bounds: (i128, i128),
    elem_w: u32,
    count: u32,
    ascending_op: bool,
    out_signed: bool,
    out_four: bool,
    loc: Loc,
) -> ValueId {
    let (total_w, _, base_four) = builder.node_shape(base_val);
    let out_w = (elem_w * count).max(1);
    let (in_bounds, bit_lo) =
        compute_packed_slice_offset(builder, idx_val, idx_signed, bounds, elem_w, count, ascending_op, total_w, loc);
    let (_, _, sh_four) = builder.node_shape(bit_lo);
    let shifted = builder.alloc_op(NetOp::LShr, vec![base_val, bit_lo], total_w, false, base_four || sh_four, loc);
    let extracted = builder.resize_val(shifted, out_w, false, out_signed, loc);

    if count <= 1 {
        let oob_cv = if out_four { CVal::all_x(out_w, out_signed) } else { CVal::new(out_w, out_signed, 0) };
        let oob = builder.alloc_const(oob_cv, out_four, loc);
        return builder.alloc_op(NetOp::Mux, vec![in_bounds, extracted, oob], out_w, out_signed, out_four, loc);
    }

    if let Some(cv) = builder.const_cval(in_bounds) {
        if cv.known() && cv.i() == 1 {
            return extracted;
        }
    }

    // Partial OOB indexed part-select: evaluate each element individually so in-bounds lanes are
    // preserved and out-of-bounds lanes become X (4-state) or 0 (2-state) per IEEE 1800 section 11.5.1.
    let (iw, _, if4) = builder.node_shape(idx_val);
    let calc_w = iw.max(32).max(addr_bits(total_w)) + 2;
    let ext_idx = builder.resize_val(idx_val, calc_w, idx_signed, true, loc);
    let lo_sv = if ascending_op {
        ext_idx
    } else {
        let span_c = builder.alloc_const(CVal::new(calc_w, true, (count - 1) as i128), false, loc);
        builder.alloc_op(NetOp::Sub, vec![ext_idx, span_c], calc_w, true, if4, loc)
    };

    let mut msb_first = Vec::with_capacity(count as usize);
    if bounds.0 >= bounds.1 {
        for k in (0..count).rev() {
            let cur_sv = if k == 0 {
                lo_sv
            } else {
                let kc = builder.alloc_const(CVal::new(calc_w, true, k as i128), false, loc);
                builder.alloc_op(NetOp::Add, vec![lo_sv, kc], calc_w, true, if4, loc)
            };
            msb_first.push(lower_dynamic_packed_slice(
                builder, base_val, cur_sv, true, bounds, elem_w, 1, true, false, out_four, loc,
            ));
        }
    } else {
        for k in 0..count {
            let cur_sv = if k == 0 {
                lo_sv
            } else {
                let kc = builder.alloc_const(CVal::new(calc_w, true, k as i128), false, loc);
                builder.alloc_op(NetOp::Add, vec![lo_sv, kc], calc_w, true, if4, loc)
            };
            msb_first.push(lower_dynamic_packed_slice(
                builder, base_val, cur_sv, true, bounds, elem_w, 1, true, false, out_four, loc,
            ));
        }
    }
    let partial_val = builder.alloc_op(NetOp::Concat, msb_first, out_w, out_signed, out_four, loc);
    builder.alloc_op(NetOp::Mux, vec![in_bounds, extracted, partial_val], out_w, out_signed, out_four, loc)
}

/// Bit-accurate read-modify-write on `old_val` writing `write_val` (`slice_w` bits) at dynamic
/// physical offset `bit_lo`, guarded by `write_en` so out-of-bounds or unknown-index writes are no-ops.
pub(crate) fn rmw_dynamic_packed_slice(
    builder: &mut NetBuilder,
    old_val: ValueId,
    write_val: ValueId,
    bit_lo: ValueId,
    slice_w: u32,
    write_en: ValueId,
    loc: Loc,
) -> ValueId {
    let write_en = coerce_two_state_en(builder, write_en, loc);
    let (total_w, s, four) = builder.node_shape(old_val);
    if slice_w >= total_w {
        let mut resized = builder.resize_val(write_val, total_w, false, s, loc);
        let (_, _, rf) = builder.node_shape(resized);
        if !four && rf {
            resized =
                builder.alloc_op(NetOp::CastFourState { four_state: false }, vec![resized], total_w, s, false, loc);
        }
        return builder.alloc_op(NetOp::Mux, vec![write_en, resized, old_val], total_w, s, four, loc);
    }
    let (_, _, lo_four) = builder.node_shape(bit_lo);
    let (_, _, w_four) = builder.node_shape(write_val);
    let comb_four = four || w_four || lo_four;

    // `mask = ((1 << slice_w) - 1) << bit_lo`
    let ones_slice = CVal::new(slice_w, false, -1).cast(total_w, false);
    let low_mask = builder.alloc_const(ones_slice, false, loc);
    let mask = builder.alloc_op(NetOp::Shl, vec![low_mask, bit_lo], total_w, false, lo_four, loc);
    let inv_mask = builder.alloc_op(NetOp::Not, vec![mask], total_w, false, lo_four, loc);
    let cleared = builder.alloc_op(NetOp::And, vec![old_val, inv_mask], total_w, false, four || lo_four, loc);

    let ext_wval = builder.resize_val(write_val, total_w, false, false, loc);
    let shifted_wval = builder.alloc_op(NetOp::Shl, vec![ext_wval, bit_lo], total_w, false, w_four || lo_four, loc);
    let masked_wval = builder.alloc_op(NetOp::And, vec![shifted_wval, mask], total_w, false, w_four || lo_four, loc);
    let mut updated = builder.alloc_op(NetOp::Or, vec![cleared, masked_wval], total_w, s, comb_four, loc);
    if !four && comb_four {
        updated = builder.alloc_op(NetOp::CastFourState { four_state: false }, vec![updated], total_w, s, false, loc);
    }

    builder.alloc_op(NetOp::Mux, vec![write_en, updated, old_val], total_w, s, four, loc)
}

/// Constant-offset bit-slice replacement `old_val[lo + slice_w - 1 : lo] := write_val`.
pub(crate) fn replace_const_packed_slice(
    builder: &mut NetBuilder,
    old_val: ValueId,
    write_val: ValueId,
    lo: u32,
    slice_w: u32,
    loc: Loc,
) -> ValueId {
    let (total_w, s, four) = builder.node_shape(old_val);
    let wval = builder.resize_val(write_val, slice_w, false, false, loc);
    if lo == 0 && slice_w == total_w {
        return builder.resize_val(wval, total_w, false, s, loc);
    }
    let mut parts = Vec::new();
    if lo + slice_w < total_w {
        parts.push(builder.alloc_op(
            NetOp::Slice { hi: total_w - 1, lo: lo + slice_w },
            vec![old_val],
            total_w - (lo + slice_w),
            false,
            four,
            loc,
        ));
    }
    parts.push(wval);
    if lo > 0 {
        parts.push(builder.alloc_op(NetOp::Slice { hi: lo - 1, lo: 0 }, vec![old_val], lo, false, four, loc));
    }
    builder.alloc_op(NetOp::Concat, parts, total_w, s, four, loc)
}

pub(crate) fn mux_obj_val(
    builder: &mut NetBuilder,
    cond: ValueId,
    then_v: ObjVal,
    else_v: ObjVal,
    ty: &RtlType,
    loc: Loc,
) -> ObjVal {
    match (then_v, else_v) {
        (ObjVal::Packed(tv), ObjVal::Packed(ev)) => {
            let (tw, ts, tf) = builder.node_shape(tv);
            let (ew, es, ef) = builder.node_shape(ev);
            let w = ty.bit_width().unwrap_or_else(|| tw.max(ew));
            let s = ty.is_signed() || (ts && es);
            let four = ty.is_four_state() || tf || ef;
            let t_res = builder.resize_val(tv, w, ts, s, loc);
            let e_res = builder.resize_val(ev, w, es, s, loc);
            ObjVal::Packed(builder.alloc_op(NetOp::Mux, vec![cond, t_res, e_res], w, s, four, loc))
        }
        (ObjVal::Struct(tf), ObjVal::Struct(ef)) => {
            let ftypes: Vec<RtlType> = match ty {
                RtlType::UnpackedStruct { fields, .. } => fields.iter().map(|(_, t)| t.clone()).collect(),
                _ => vec![RtlType::Void; tf.len()],
            };
            let mut out = Vec::with_capacity(tf.len());
            for (((name, tv), (_, ev)), fty) in tf.into_iter().zip(ef).zip(ftypes) {
                out.push((name, mux_obj_val(builder, cond, tv, ev, &fty, loc)));
            }
            ObjVal::Struct(out)
        }
        (ObjVal::Elems { left, right, elems: te }, ObjVal::Elems { elems: ee, .. }) => {
            let sub_ty = match ty {
                RtlType::UnpackedArray { elem, dims } if dims.len() > 1 => {
                    RtlType::UnpackedArray { elem: elem.clone(), dims: dims[1..].to_vec() }
                }
                RtlType::UnpackedArray { elem, .. } => (**elem).clone(),
                _ => RtlType::Void,
            };
            let elems = te.into_iter().zip(ee).map(|(a, b)| mux_obj_val(builder, cond, a, b, &sub_ty, loc)).collect();
            ObjVal::Elems { left, right, elems }
        }
        (t, _) => t,
    }
}

pub(crate) fn make_oob_obj_val(builder: &mut NetBuilder, ty: &RtlType, loc: Loc) -> ObjVal {
    if let Some((w, s, four)) = ty.integral_shape() {
        let cv = if four { CVal::all_x(w, s) } else { CVal::new(w, s, 0) };
        return ObjVal::Packed(builder.alloc_const(cv, four, loc));
    }
    match ty {
        RtlType::UnpackedStruct { fields, .. } => {
            ObjVal::Struct(fields.iter().map(|(n, ft)| (n.clone(), make_oob_obj_val(builder, ft, loc))).collect())
        }
        RtlType::UnpackedArray { elem, dims } if !dims.is_empty() => {
            let (left, right) = dims[0];
            let cnt = ((left - right).unsigned_abs() as usize) + 1;
            let sub_ty = if dims.len() > 1 {
                RtlType::UnpackedArray { elem: elem.clone(), dims: dims[1..].to_vec() }
            } else {
                (**elem).clone()
            };
            let elems = (0..cnt).map(|_| make_oob_obj_val(builder, &sub_ty, loc)).collect();
            ObjVal::Elems { left, right, elems }
        }
        _ => ObjVal::Packed(builder.alloc_const(CVal::new(1, false, 0), false, loc)),
    }
}

fn lower_dynamic_clog2(builder: &mut NetBuilder, arg: ValueId, loc: Loc) -> ValueId {
    let (w, _, four) = builder.node_shape(arg);
    // `$clog2(x)` returns `0` for `x <= 1`, otherwise the smallest `k` in `1..=w` with `(x - 1) >> k == 0`.
    let ext_w = w + 1;
    let x_u = builder.resize_val(arg, ext_w, false, false, loc);
    let one = builder.alloc_const(CVal::new(ext_w, false, 1), false, loc);
    let le_one = builder.alloc_op(NetOp::ULe, vec![x_u, one], 1, false, four, loc);
    let xm1 = builder.alloc_op(NetOp::Sub, vec![x_u, one], ext_w, false, four, loc);

    // MSB priority encoder on `xm1[w-1 : 0]`: when `xm1[k]` is the highest set bit, `$clog2(x) == k + 1`.
    let mut acc = builder.alloc_const(CVal::new(32, true, 0), false, loc);
    for k in 0..w {
        let bit_k = builder.alloc_op(NetOp::Slice { hi: k, lo: k }, vec![xm1], 1, false, four, loc);
        let res_k = builder.alloc_const(CVal::new(32, true, (k + 1) as i128), false, loc);
        acc = builder.alloc_op(NetOp::Mux, vec![bit_k, res_k, acc], 32, true, four, loc);
    }
    let zero32 = builder.alloc_const(CVal::new(32, true, 0), false, loc);
    builder.alloc_op(NetOp::Mux, vec![le_one, zero32, acc], 32, true, four, loc)
}
