//! Hierarchy flattening, signal/memory/port slot allocation, port stitching,
//! continuous assignments, gates, and SVA property attachment.

use sv_semantic::expr::resolve_interface_port;
use sv_semantic::fx::{FxMap, FxSet};
use sv_semantic::{
    BoundProperty, CVal, ElabItemKind, ElaboratedDesign, ExprArena, ExprBinder, ExprId, ExprKind, InstanceId, Loc,
    PortConn, PortDir, PropExpr, RtlType, SeqExpr, SvaBinder, SvaClock, SvaEdge, SymbolId, SymbolKind,
};
use sv_syntax::{Child, NodeId, SyntaxKind, Tree};

use crate::builder::{BuildKind, NetBuilder};
use crate::drivers::{resolve_all_signals, DriverKind, SignalSlot};
use crate::expr_lower::{
    addr_bits, compute_packed_slice_offset, compute_unpacked_index_bounds, extract_lsb_bit0, ExprLowerer, FuncInliner,
    ObjSlot, ObjVal,
};
use crate::ir::{
    ClockSpec, Edge, LowerConfig, LowerOutcome, MemId, Memory, NetOp, Netlist, NetlistDiag, PortInput, PortOutput,
    TraceSignal, ValueId, ValueKind, ValueNode,
};

#[derive(Clone, Debug)]
pub(crate) struct PendingAlias {
    pub sidx_a: usize,
    pub lo_a: u32,
    pub sidx_b: usize,
    pub lo_b: u32,
    pub width: u32,
    pub loc: Loc,
}

pub(crate) struct DesignLowerer<'a> {
    pub trees: &'a [Tree],
    pub design: &'a ElaboratedDesign,
    pub config: &'a LowerConfig,
    pub builder: NetBuilder,
    pub slots: Vec<SignalSlot>,
    pub slot_placeholders: Vec<ValueId>,
    pub sym_slots: FxMap<SymbolId, ObjSlot>,
    pub eligible_memories: FxSet<SymbolId>,
    pub pending_aliases: Vec<PendingAlias>,
    pub next_local_id: u32,
    pub call_stack: Vec<(usize, NodeId)>,
}

/// Lowers an [`ElaboratedDesign`] and its parsed CST [`Tree`]s into a word-level [`Netlist`].
///
/// Selecting the root instance follows [`LowerConfig::top`] when set, or requires [`ElaboratedDesign::tops`]
/// to contain a single elaborated top module. Any illegal SystemVerilog construct yields
/// [`LowerOutcome::Error`], while synthesizable-subset or configuration limits yield
/// [`LowerOutcome::Unsupported`].
pub fn lower_design(trees: &[Tree], design: &ElaboratedDesign, config: &LowerConfig) -> LowerOutcome {
    let top_id = match select_top(design, config) {
        Ok(id) => id,
        Err(diag) => return LowerOutcome::Error(vec![diag]),
    };
    let top_name = design.instance(top_id).definition.clone();
    let eligible_memories = prescan_eligible_memories(trees, design, top_id);
    let mut lowerer = DesignLowerer {
        trees,
        design,
        config,
        builder: NetBuilder::new(top_name, design.files.clone()),
        slots: Vec::new(),
        slot_placeholders: Vec::new(),
        sym_slots: FxMap::default(),
        eligible_memories,
        pending_aliases: Vec::new(),
        next_local_id: 0,
        call_stack: Vec::new(),
    };

    // Pass 1: allocate flattened signal slots, top-level ports, memories, and trace entries.
    lowerer.alloc_instance_recursive(top_id, true);
    if !lowerer.builder.diags.is_empty() {
        return classify_diags(lowerer.builder.diags);
    }

    // Pass 2: record declaration initializers and `initial` block constant values.
    lowerer.collect_initializers_recursive(top_id);
    if !lowerer.builder.diags.is_empty() {
        return classify_diags(lowerer.builder.diags);
    }

    // Pass 3: lower port connections, continuous assigns, gates, net aliases, and procedural blocks.
    lowerer.lower_instance_recursive(top_id);
    lowerer.resolve_pending_aliases();
    if !lowerer.builder.diags.is_empty() {
        return classify_diags(lowerer.builder.diags);
    }

    // Pass 4: bind concurrent SVA properties across all elaborated instances and lower their leaf expressions.
    let mut sva_arena = ExprArena::new();
    let mut properties = Vec::new();
    lowerer.collect_sva_recursive(top_id, &mut sva_arena, &mut properties);
    if !lowerer.builder.diags.is_empty() {
        return classify_diags(lowerer.builder.diags);
    }
    let prop_expr_values = lowerer.lower_sva_properties(&sva_arena, &properties);
    if !lowerer.builder.diags.is_empty() {
        return classify_diags(lowerer.builder.diags);
    }

    // Pass 5: bit-level driver interval resolution (multi-driver slice merging, overlap errors, undriven bits).
    resolve_all_signals(&lowerer.slots, &mut lowerer.builder);
    if !lowerer.builder.diags.is_empty() {
        return classify_diags(lowerer.builder.diags);
    }

    let mut symbol_values: Vec<(SymbolId, ValueId)> = lowerer
        .sym_slots
        .iter()
        .filter_map(|(&sid, slot)| match slot {
            ObjSlot::Packed(sidx) => Some((sid, lowerer.slot_placeholders[*sidx])),
            _ => None,
        })
        .collect();
    symbol_values.sort_by_key(|(sid, _)| sid.0);

    // Pass 6: Tarjan SCC combinational loop check, alias resolution, and topological compaction.
    match lowerer.builder.finalize(sva_arena, properties, prop_expr_values, symbol_values) {
        Ok(mut netlist) => {
            if let Some(max_depth) = config.blast_memories_up_to {
                netlist.blast_memories(max_depth);
            }
            LowerOutcome::Lowered(netlist)
        }
        Err(diags) => classify_diags(diags),
    }
}

pub(crate) fn classify_diags(diags: Vec<NetlistDiag>) -> LowerOutcome {
    if diags.iter().any(|d| !d.unsupported) {
        LowerOutcome::Error(diags)
    } else {
        LowerOutcome::Unsupported(diags)
    }
}

fn select_top(design: &ElaboratedDesign, config: &LowerConfig) -> Result<InstanceId, NetlistDiag> {
    if design.tops.is_empty() {
        return Err(NetlistDiag {
            loc: Loc::default(),
            file: String::new(),
            message: "no top-level module found to lower".into(),
            unsupported: false,
        });
    }
    if let Some(req) = &config.top {
        for &tid in &design.tops {
            let inst = design.instance(tid);
            if &inst.definition == req || &inst.name == req {
                return Ok(tid);
            }
        }
        return Err(NetlistDiag {
            loc: Loc::default(),
            file: String::new(),
            message: format!("requested top module '{req}' was not elaborated"),
            unsupported: false,
        });
    }
    if design.tops.len() > 1 {
        return Err(NetlistDiag {
            loc: Loc::default(),
            file: String::new(),
            message: "multiple top modules elaborated; specify LowerConfig::top".into(),
            unsupported: false,
        });
    }
    Ok(design.tops[0])
}

impl<'a> DesignLowerer<'a> {
    pub fn alloc_slot(
        &mut self,
        name: String,
        width: u32,
        signed: bool,
        four_state: bool,
        is_net: bool,
        loc: Loc,
    ) -> usize {
        let placeholder = self.builder.alloc_placeholder(width, signed, four_state, Some(name.clone()), loc);
        let idx = self.slots.len();
        self.slots.push(SignalSlot {
            name: name.clone(),
            width,
            signed,
            four_state,
            is_net,
            placeholder,
            init: None,
            loc,
            drivers: Vec::new(),
        });
        self.slot_placeholders.push(placeholder);
        self.builder.traces.push(TraceSignal { name, width, signed, four_state, value: placeholder, loc });
        idx
    }

    fn alloc_instance_recursive(&mut self, inst_id: InstanceId, is_top: bool) {
        let inst = self.design.instance(inst_id);
        let syms = inst.symbols.clone();
        let children = inst.children.clone();

        for sid in syms {
            let sym = self.design.symbol(sid);
            if let RtlType::NonSynthesizable(reason) = &sym.ty {
                self.builder.err(sym.loc, format!("signal '{}' has non-synthesizable type '{reason}'", sym.full_path));
                continue;
            }
            if matches!(sym.ty, RtlType::Event) {
                self.builder.unsup(sym.loc, format!("named event '{}' is not synthesizable in RTL", sym.full_path));
                continue;
            }
            if matches!(sym.ty, RtlType::Interface { .. }) {
                // Interface port handles resolve through `resolve_interface_port` to concrete interface symbols.
                continue;
            }

            match &sym.kind {
                SymbolKind::Param { .. } | SymbolKind::Genvar => {
                    let Some(cv) = sym.cval.clone() else {
                        self.builder.err(sym.loc, format!("parameter '{}' has no constant value", sym.full_path));
                        continue;
                    };
                    self.sym_slots.insert(sid, ObjSlot::Const(cv, sym.ty.is_four_state()));
                }
                SymbolKind::Port { dir, is_net } => {
                    if matches!(dir, PortDir::Inout | PortDir::Ref) {
                        self.builder.unsup(
                            sym.loc,
                            format!("{} port '{}' is not supported in RTL netlist", dir.as_str(), sym.full_path),
                        );
                        continue;
                    }
                    let port_is_net = if is_top && *dir == PortDir::Input { false } else { *is_net };
                    let top_dir = is_top.then_some(*dir);
                    if let Some(obj_slot) = self.alloc_type_slot(
                        Some(sid),
                        &sym.full_path,
                        &sym.name,
                        &sym.ty,
                        port_is_net,
                        top_dir,
                        sym.loc,
                    ) {
                        self.sym_slots.insert(sid, obj_slot);
                    }
                }
                SymbolKind::Var => {
                    if let Some(obj_slot) =
                        self.alloc_type_slot(Some(sid), &sym.full_path, &sym.name, &sym.ty, false, None, sym.loc)
                    {
                        self.sym_slots.insert(sid, obj_slot);
                    }
                }
                SymbolKind::Net { net_type } => {
                    if matches!(net_type.as_str(), "tri0" | "tri1" | "trireg" | "wand" | "wor" | "triand" | "trior") {
                        self.builder.unsup(
                            sym.loc,
                            format!("multi-resolution net type '{net_type}' on '{}' is not supported", sym.full_path),
                        );
                        continue;
                    }
                    if let Some(obj_slot) =
                        self.alloc_type_slot(Some(sid), &sym.full_path, &sym.name, &sym.ty, true, None, sym.loc)
                    {
                        self.sym_slots.insert(sid, obj_slot);
                    }
                }
            }
        }

        for cid in children {
            self.alloc_instance_recursive(cid, false);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn alloc_type_slot(
        &mut self,
        sym_id: Option<SymbolId>,
        full_path: &str,
        port_rel_name: &str,
        ty: &RtlType,
        is_net: bool,
        top_port_dir: Option<PortDir>,
        loc: Loc,
    ) -> Option<ObjSlot> {
        if !validate_rtl_type(&mut self.builder, full_path, ty, loc) {
            return None;
        }
        if let Some((w, s, four)) = ty.integral_shape() {
            let sidx = self.alloc_slot(full_path.to_string(), w, s, four, is_net, loc);
            if let Some(dir) = top_port_dir {
                match dir {
                    PortDir::Input => {
                        let pidx = self.builder.inputs.len();
                        let in_val = self.builder.alloc_raw(
                            w,
                            s,
                            four,
                            BuildKind::Input(pidx),
                            Some(port_rel_name.to_string()),
                            loc,
                        );
                        self.builder.inputs.push(PortInput {
                            name: port_rel_name.to_string(),
                            width: w,
                            signed: s,
                            four_state: four,
                            value: in_val,
                            loc,
                        });
                        self.slots[sidx].add_driver(0, w - 1, in_val, DriverKind::InputPort, loc);
                    }
                    PortDir::Output => {
                        let out_val = self.slot_placeholders[sidx];
                        self.builder.outputs.push(PortOutput {
                            name: port_rel_name.to_string(),
                            width: w,
                            signed: s,
                            four_state: four,
                            value: out_val,
                            loc,
                        });
                    }
                    _ => {}
                }
            }
            return Some(ObjSlot::Packed(sidx));
        }

        match ty {
            RtlType::UnpackedStruct { fields, .. } => {
                let mut out_fields = Vec::with_capacity(fields.len());
                for (fname, fty) in fields {
                    let sub_full = format!("{full_path}.{fname}");
                    let sub_port = format!("{port_rel_name}.{fname}");
                    let sub_slot = self.alloc_type_slot(None, &sub_full, &sub_port, fty, is_net, top_port_dir, loc)?;
                    out_fields.push((fname.clone(), sub_slot));
                }
                Some(ObjSlot::Struct(out_fields))
            }
            RtlType::UnpackedArray { elem, dims } if !dims.is_empty() => {
                let (left, right) = dims[0];
                let min_b = left.min(right);
                let max_b = left.max(right);
                let depth_u128 = (max_b - min_b).unsigned_abs() + 1;
                if depth_u128 > u32::MAX as u128 {
                    self.builder.unsup(loc, format!("unpacked array '{full_path}' exceeds maximum depth"));
                    return None;
                }
                let depth = depth_u128 as u32;

                if sym_id.is_some_and(|s| self.eligible_memories.contains(&s))
                    && top_port_dir.is_none()
                    && !is_net
                    && dims.len() == 1
                {
                    if let Some((w, s, four)) = elem.integral_shape() {
                        let mid = MemId(self.builder.memories.len() as u32);
                        self.builder.memories.push(Memory {
                            id: mid,
                            name: full_path.to_string(),
                            width: w,
                            depth,
                            lo_idx: min_b,
                            hi_idx: max_b,
                            signed: s,
                            four_state: four,
                            init: Vec::new(),
                            writes: Vec::new(),
                            loc,
                        });
                        return Some(ObjSlot::Memory(mid));
                    }
                }

                let total_elems = total_unpacked_elems(ty);
                if total_elems.is_none_or(|n| n > 4096) {
                    self.builder
                        .unsup(loc, format!("scalarized unpacked array '{full_path}' exceeds maximum 4096 elements"));
                    return None;
                }

                let sub_ty = if dims.len() > 1 {
                    RtlType::UnpackedArray { elem: elem.clone(), dims: dims[1..].to_vec() }
                } else {
                    (**elem).clone()
                };
                let mut elems = Vec::with_capacity(depth as usize);
                for offset in 0..(depth as i128) {
                    let idx = min_b + offset;
                    let sub_full = format!("{full_path}[{idx}]");
                    let sub_port = format!("{port_rel_name}[{idx}]");
                    elems.push(self.alloc_type_slot(None, &sub_full, &sub_port, &sub_ty, is_net, top_port_dir, loc)?);
                }
                Some(ObjSlot::Elems { left, right, elems })
            }
            _ => {
                self.builder.unsup(loc, format!("unsupported signal type on '{full_path}'"));
                None
            }
        }
    }

    fn collect_initializers_recursive(&mut self, inst_id: InstanceId) {
        let inst = self.design.instance(inst_id);
        let syms = inst.symbols.clone();
        let items = inst.items.clone();
        let children = inst.children.clone();

        // 1. Declaration initializers (`logic [3:0] q = 4'd1;`).
        for sid in syms {
            let sym = self.design.symbol(sid);
            if matches!(sym.kind, SymbolKind::Param { .. } | SymbolKind::Genvar) {
                continue;
            }
            if let SymbolKind::Net { net_type } = &sym.kind {
                if net_type == "supply0" || net_type == "supply1" {
                    if let Some(ObjSlot::Packed(sidx)) = self.sym_slots.get(&sid).cloned() {
                        let w = self.slots[sidx].width;
                        let cv = CVal::new(w, false, if net_type == "supply1" { -1 } else { 0 });
                        let v = self.builder.alloc_const(cv, true, sym.loc);
                        self.slots[sidx].add_driver(0, w - 1, v, DriverKind::ContinuousAssign, sym.loc);
                    }
                }
            }
            let Some((init_tree, init_node)) = sym.init_node else { continue };
            let Some(slot) = self.sym_slots.get(&sid).cloned() else { continue };
            if sym.is_net_with_init() {
                // `wire [3:0] w = a + b;` is an implicit continuous assignment per IEEE 1800 §10.3.1!
                self.lower_net_decl_assign(inst_id, sid, &slot, init_tree, init_node, &sym.init_scope_prefix, sym.loc);
                continue;
            }
            // Variable declaration initializer: record constant initial value!
            if let Some(cv) = &sym.cval {
                self.apply_const_init_to_slot(&slot, cv);
            } else {
                self.eval_decl_init_expr(inst_id, &slot, &sym.ty, init_tree, init_node, &sym.init_scope_prefix);
            }
        }

        // 2. `initial` blocks (`initial q = 4'd5;`, `initial begin ... end`).
        for item in items {
            if item.kind == ElabItemKind::ProceduralBlock {
                let t = &self.trees[item.tree];
                let kw = own_first_tok(t, item.node).unwrap_or("");
                if kw == "initial" {
                    self.exec_initial_block(inst_id, &item);
                }
            }
        }

        for cid in children {
            self.collect_initializers_recursive(cid);
        }
    }

    fn apply_const_init_to_slot(&mut self, slot: &ObjSlot, cv: &CVal) {
        if let ObjSlot::Packed(sidx) = slot {
            let w = self.slots[*sidx].width;
            let s = self.slots[*sidx].signed;
            self.slots[*sidx].init = Some(cv.cast(w, s));
        }
    }

    fn eval_decl_init_expr(
        &mut self,
        inst_id: InstanceId,
        slot: &ObjSlot,
        ty: &RtlType,
        tree_idx: usize,
        node: NodeId,
        scope_prefix: &str,
    ) {
        let gen_consts = self.collect_scope_gen_consts(inst_id, scope_prefix);
        let mut arena = ExprArena::new();
        let mut binder = ExprBinder::new(self.trees, self.design, inst_id, scope_prefix, &gen_consts, &mut arena);
        let loc = binder.loc_of(tree_idx, node);
        let Some(eid) = binder.bind_assigned(tree_idx, node, ty) else {
            if binder.diags.is_empty() {
                self.builder.unsup(loc, "unsupported variable declaration initializer");
            } else {
                self.flush_expr_diags(binder.diags);
            }
            return;
        };
        self.flush_expr_diags(binder.diags);
        self.apply_bound_init_to_slot(&arena, eid, slot, ty);
    }

    pub(crate) fn apply_bound_init_to_slot(&mut self, arena: &ExprArena, eid: ExprId, slot: &ObjSlot, ty: &RtlType) {
        let be = arena.get(eid);
        if let Some(cv) = &be.cval {
            self.apply_const_init_to_slot(slot, cv);
            return;
        }
        if let ExprKind::AssignPattern(entries) = &be.kind {
            match slot {
                ObjSlot::Struct(fields) => {
                    let ty_fields = match ty {
                        RtlType::UnpackedStruct { fields: tf, .. } => tf.as_slice(),
                        _ => &[],
                    };
                    for (idx, (fname, fslot)) in fields.iter().enumerate() {
                        let fty = ty_fields.iter().find(|(n, _)| n == fname).map(|(_, t)| t).unwrap_or(&RtlType::Void);
                        if let Some((_, sub_e)) = entries
                            .iter()
                            .find(|(k, _)| k.as_deref() == Some(fname.as_str()))
                            .or_else(|| entries.get(idx))
                        {
                            self.apply_bound_init_to_slot(arena, *sub_e, fslot, fty);
                        } else {
                            self.builder.unsup(be.loc, format!("missing field '{fname}' in declaration initializer"));
                        }
                    }
                }
                ObjSlot::Elems { left, right, elems } => {
                    let sub_ty = match ty {
                        RtlType::UnpackedArray { elem, dims } if dims.len() > 1 => {
                            RtlType::UnpackedArray { elem: elem.clone(), dims: dims[1..].to_vec() }
                        }
                        RtlType::UnpackedArray { elem, .. } => (**elem).clone(),
                        _ => RtlType::Void,
                    };
                    let rev = *left > *right;
                    for (idx, eslot) in elems.iter().enumerate() {
                        let src_idx = if rev { entries.len().saturating_sub(1 + idx) } else { idx };
                        if let Some((_, sub_e)) = entries.get(src_idx) {
                            self.apply_bound_init_to_slot(arena, *sub_e, eslot, &sub_ty);
                        } else {
                            self.builder.unsup(be.loc, "insufficient elements in array declaration initializer");
                        }
                    }
                }
                ObjSlot::Memory(mid) => {
                    let rev =
                        matches!(ty, RtlType::UnpackedArray { dims, .. } if dims.first().is_some_and(|(l, r)| l > r));
                    let m_depth = self.builder.memories[mid.0 as usize].depth;
                    for (idx, (_, sub_e)) in entries.iter().enumerate() {
                        let sub_node = arena.get(*sub_e);
                        let Some(cv) = &sub_node.cval else {
                            self.builder.unsup(
                                sub_node.loc,
                                "non-constant element in memory declaration initializer is not supported",
                            );
                            return;
                        };
                        let m = &mut self.builder.memories[mid.0 as usize];
                        if (idx as u32) < m.depth {
                            let phys_off = if rev { (m_depth - 1) - (idx as u32) } else { idx as u32 };
                            let casted = cv.cast(m.width, m.signed);
                            m.init.retain(|(i, _)| *i != phys_off);
                            m.init.push((phys_off, casted));
                        }
                    }
                    self.builder.memories[mid.0 as usize].init.sort_by_key(|(off, _)| *off);
                }
                _ => {
                    self.builder.unsup(be.loc, "unsupported aggregate declaration initializer");
                }
            }
            return;
        }
        self.builder.unsup(be.loc, "non-constant variable declaration initializer is not supported in RTL netlist");
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_net_decl_assign(
        &mut self,
        inst_id: InstanceId,
        sid: SymbolId,
        slot: &ObjSlot,
        tree_idx: usize,
        init_node: NodeId,
        scope_prefix: &str,
        loc: Loc,
    ) {
        let ty = self.design.symbol(sid).ty.clone();
        let gen_consts = self.collect_scope_gen_consts(inst_id, scope_prefix);
        let mut arena = ExprArena::new();
        let mut binder = ExprBinder::new(self.trees, self.design, inst_id, scope_prefix, &gen_consts, &mut arena);
        let Some(eid) = binder.bind_assigned(tree_idx, init_node, &ty) else {
            self.flush_expr_diags(binder.diags);
            return;
        };
        self.flush_expr_diags(binder.diags);
        if self.reject_tristate_z(&arena, eid) {
            return;
        }
        let Some(rhs_val) = self.eval_comb_expr(&arena, eid) else { return };
        self.drive_obj_slot(slot, rhs_val, DriverKind::ContinuousAssign, loc);
    }

    fn lower_instance_recursive(&mut self, inst_id: InstanceId) {
        let inst = self.design.instance(inst_id);
        let items = inst.items.clone();
        let children = inst.children.clone();

        // 1. Stitch ports for every child instance.
        for &cid in &children {
            self.stitch_child_ports(inst_id, cid);
        }

        // 2. Lower active items in this instance.
        for item in &items {
            match item.kind {
                ElabItemKind::ContinuousAssign => self.lower_continuous_assign(inst_id, item),
                ElabItemKind::ProceduralBlock => self.lower_procedural_block(inst_id, item),
                ElabItemKind::AssertStmt => self.lower_module_immediate_assert(inst_id, item),
                ElabItemKind::GateInst => self.lower_gate_inst(inst_id, item),
                ElabItemKind::NetAlias => self.lower_net_alias(inst_id, item),
                _ => {}
            }
        }

        // 3. Recurse into children.
        for cid in children {
            self.lower_instance_recursive(cid);
        }
    }

    fn stitch_child_ports(&mut self, parent_id: InstanceId, child_id: InstanceId) {
        let child = self.design.instance(child_id);
        let ports = child.ports.clone();
        let conns = child.port_conns.clone();
        let inst_loc = child.loc;

        for (port_idx, port_sid) in ports.into_iter().enumerate() {
            let port_sym = self.design.symbol(port_sid);
            if matches!(port_sym.ty, RtlType::Interface { .. }) {
                continue;
            }
            let SymbolKind::Port { dir, .. } = port_sym.kind else { continue };
            let port_name = port_sym.name.clone();
            let port_ty = port_sym.ty.clone();
            let port_loc = port_sym.loc;
            let port_default = port_sym.init_node;
            let port_default_prefix = port_sym.init_scope_prefix.clone();

            let conn_opt: Option<PortConn> = conns
                .iter()
                .find(|c| c.port_name.as_deref() == Some(port_name.as_str()))
                .or_else(|| conns.iter().find(|c| c.port_name.is_none() && !c.wildcard && c.arg_index == port_idx))
                .or_else(|| conns.iter().find(|c| c.wildcard))
                .cloned();

            let Some(child_slot) = self.sym_slots.get(&port_sid).cloned() else { continue };

            match dir {
                PortDir::Input => {
                    match conn_opt {
                        Some(conn) if conn.implicit_named || conn.wildcard => {
                            let parent_inst = self.design.instance(parent_id);
                            if let Some(psid) = parent_inst.resolve_lexical(&conn.scope_prefix, &port_name) {
                                // Synthesize bound reference to `psid`, coerced to `port_ty`!
                                let mut arena = ExprArena::new();
                                let psym = self.design.symbol(psid);
                                let raw_id =
                                    arena.alloc(ExprKind::Symbol(psid), psym.ty.clone(), psym.cval.clone(), conn.loc);
                                let bound_id = if let Some((pw, ps, p4)) = port_ty.integral_shape() {
                                    arena.coerce(raw_id, pw, ps, p4, conn.loc)
                                } else {
                                    raw_id
                                };
                                if let Some(val) = self.eval_comb_expr(&arena, bound_id) {
                                    self.drive_obj_slot(&child_slot, val, DriverKind::InputPort, conn.loc);
                                }
                            } else if conn.wildcard && port_default.is_some() {
                                if let Some((dt, dn)) = port_default {
                                    self.drive_child_input_from_default(
                                        child_id,
                                        &child_slot,
                                        &port_ty,
                                        dt,
                                        dn,
                                        &port_default_prefix,
                                        port_loc,
                                    );
                                }
                            } else {
                                self.builder.err(
                                    conn.loc,
                                    format!("undeclared identifier '{port_name}' in implicit/wildcard port connection"),
                                );
                            }
                        }
                        Some(conn) if conn.expr.is_some() => {
                            let (tr, nd) = conn.expr.expect("checked Some");
                            let mut arena = ExprArena::new();
                            let mut binder = ExprBinder::new(
                                self.trees,
                                self.design,
                                parent_id,
                                &conn.scope_prefix,
                                &conn.gen_consts,
                                &mut arena,
                            );
                            let eid_opt = binder.bind_assigned(tr, nd, &port_ty);
                            self.flush_expr_diags(binder.diags);
                            if let Some(eid) = eid_opt {
                                if let Some(val) = self.eval_comb_expr(&arena, eid) {
                                    self.drive_obj_slot(&child_slot, val, DriverKind::InputPort, conn.loc);
                                }
                            }
                        }
                        _ => {
                            // Unconnected `.p()` or omitted port: apply default value if declared!
                            if let Some((dt, dn)) = port_default {
                                self.drive_child_input_from_default(
                                    child_id,
                                    &child_slot,
                                    &port_ty,
                                    dt,
                                    dn,
                                    &port_default_prefix,
                                    inst_loc,
                                );
                            }
                        }
                    }
                }
                PortDir::Output => {
                    let child_val = self.read_slot_obj(&child_slot, port_loc);
                    match conn_opt {
                        Some(conn) if conn.implicit_named || conn.wildcard => {
                            let parent_inst = self.design.instance(parent_id);
                            if let Some(psid) = parent_inst.resolve_lexical(&conn.scope_prefix, &port_name) {
                                if let Some(pslot) = self.sym_slots.get(&psid).cloned() {
                                    let pty = self.design.symbol(psid).ty.clone();
                                    let coerced = self.coerce_obj_val(child_val, &port_ty, &pty, conn.loc);
                                    self.drive_obj_slot(&pslot, coerced, DriverKind::ChildOutput, conn.loc);
                                }
                            } else if !conn.wildcard {
                                self.builder.err(
                                    conn.loc,
                                    format!("undeclared identifier '{port_name}' in implicit port connection"),
                                );
                            }
                        }
                        Some(conn) if conn.expr.is_some() => {
                            let (tr, nd) = conn.expr.expect("checked Some");
                            self.drive_lhs_expr_continuous(
                                parent_id,
                                tr,
                                nd,
                                &conn.scope_prefix,
                                &conn.gen_consts,
                                child_val,
                                &port_ty,
                                DriverKind::ChildOutput,
                                conn.loc,
                            );
                        }
                        _ => {
                            // Explicitly unconnected output `.out()` is allowed in Verilog and SystemVerilog.
                        }
                    }
                }
                _ => {}
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn drive_child_input_from_default(
        &mut self,
        child_id: InstanceId,
        child_slot: &ObjSlot,
        port_ty: &RtlType,
        tree_idx: usize,
        node: NodeId,
        scope_prefix: &str,
        loc: Loc,
    ) {
        let gen_consts = self.collect_scope_gen_consts(child_id, scope_prefix);
        let mut arena = ExprArena::new();
        let mut binder = ExprBinder::new(self.trees, self.design, child_id, scope_prefix, &gen_consts, &mut arena);
        let eid_opt = binder.bind_assigned(tree_idx, node, port_ty);
        self.flush_expr_diags(binder.diags);
        if let Some(eid) = eid_opt {
            if let Some(val) = self.eval_comb_expr(&arena, eid) {
                self.drive_obj_slot(child_slot, val, DriverKind::InputPort, loc);
            }
        }
    }

    fn lower_continuous_assign(&mut self, inst_id: InstanceId, item: &sv_semantic::ElabItem) {
        let t = &self.trees[item.tree];
        if t.child(item.node, SyntaxKind::TimingControl).is_some() {
            // Reject non-synthesizable continuous assignment delay fail-closed (`unsupported`).
            self.builder.unsup(item.loc, "continuous assignment propagation delay '#d' is not supported");
            return;
        }
        for assign_expr in t.children(item.node).filter(|&c| t.kind(c) == SyntaxKind::BinaryExpr) {
            let kids: Vec<NodeId> = t.children(assign_expr).filter(|&c| t.kind(c) != SyntaxKind::Attribute).collect();
            if kids.len() != 2 {
                continue;
            }
            let (lhs_node, rhs_node) = (kids[0], kids[1]);
            let mut arena = ExprArena::new();
            let mut binder =
                ExprBinder::new(self.trees, self.design, inst_id, &item.scope_prefix, &item.gen_consts, &mut arena);
            let loc = binder.loc_of(item.tree, assign_expr);
            self.check_modport_lhs_legality(inst_id, item.tree, lhs_node, &item.scope_prefix, loc);
            let Some(lhs_eid) = binder.bind_self(item.tree, lhs_node) else {
                self.flush_expr_diags(binder.diags);
                continue;
            };
            let lhs_ty = binder.arena.get(lhs_eid).ty.clone();
            let Some(rhs_eid) = binder.bind_assigned(item.tree, rhs_node, &lhs_ty) else {
                self.flush_expr_diags(binder.diags);
                continue;
            };
            self.flush_expr_diags(binder.diags);
            if self.reject_tristate_z(&arena, rhs_eid) {
                continue;
            }
            let Some(rhs_val) = self.eval_comb_expr(&arena, rhs_eid) else { continue };
            self.drive_bound_lhs_continuous(&arena, lhs_eid, rhs_val, DriverKind::ContinuousAssign, loc);
        }
    }

    fn lower_gate_inst(&mut self, inst_id: InstanceId, item: &sv_semantic::ElabItem) {
        let t = &self.trees[item.tree];
        let gate_kw = own_first_tok(t, item.node).unwrap_or("");
        if !matches!(gate_kw, "and" | "nand" | "or" | "nor" | "xor" | "xnor" | "buf" | "not") {
            self.builder.unsup(item.loc, format!("switch/tri-state gate primitive '{gate_kw}' is not supported"));
            return;
        }
        if t.child(item.node, SyntaxKind::TimingControl).is_some() {
            self.builder.unsup(item.loc, "gate propagation delay is not supported");
            return;
        }
        let instances: Vec<NodeId> = t.children(item.node).filter(|&c| t.kind(c) == SyntaxKind::Instance).collect();
        for inst_node in &instances {
            if t.child(*inst_node, SyntaxKind::Dim).is_some() {
                self.builder.unsup(item.loc, "gate instance array is not supported");
                return;
            }
        }
        let arg_lists: Vec<NodeId> =
            subtree_preorder(t, item.node).into_iter().filter(|&n| t.kind(n) == SyntaxKind::ArgList).collect();
        for al in arg_lists {
            let args: Vec<NodeId> = t
                .children(al)
                .filter(|&c| t.kind(c) != SyntaxKind::Attribute)
                .filter_map(|c| {
                    if t.kind(c) == SyntaxKind::Arg {
                        t.children(c).find(|&k| t.kind(k) != SyntaxKind::Attribute)
                    } else {
                        Some(c)
                    }
                })
                .collect();
            if args.len() < 2 {
                self.builder.err(item.loc, format!("gate primitive '{gate_kw}' requires at least 2 terminals"));
                continue;
            }
            let mut arena = ExprArena::new();
            let mut binder =
                ExprBinder::new(self.trees, self.design, inst_id, &item.scope_prefix, &item.gen_consts, &mut arena);
            let loc = binder.loc_of(item.tree, al);
            if matches!(gate_kw, "buf" | "not") {
                let in_node = *args.last().expect("len >= 2");
                let Some(in_e) = binder.bind_condition(item.tree, in_node) else {
                    self.flush_expr_diags(binder.diags);
                    continue;
                };
                let mut out_es = Vec::new();
                for &on in &args[..args.len() - 1] {
                    if let Some(oe) = binder.bind_self(item.tree, on) {
                        out_es.push(oe);
                    }
                }
                self.flush_expr_diags(binder.diags);
                if out_es.len() != args.len() - 1 {
                    continue;
                }
                let Some(in_v) = self.eval_comb_expr(&arena, in_e).and_then(|o| o.as_packed()) else { continue };
                let res_v = if gate_kw == "not" {
                    self.builder.alloc_op(NetOp::Not, vec![in_v], 1, false, true, loc)
                } else {
                    in_v
                };
                for oe in out_es {
                    self.drive_bound_lhs_continuous(
                        &arena,
                        oe,
                        ObjVal::Packed(res_v),
                        DriverKind::ContinuousAssign,
                        loc,
                    );
                }
            } else {
                let Some(out_e) = binder.bind_self(item.tree, args[0]) else {
                    self.flush_expr_diags(binder.diags);
                    continue;
                };
                let mut in_es = Vec::new();
                for &inp in &args[1..] {
                    if let Some(ie) = binder.bind_condition(item.tree, inp) {
                        in_es.push(ie);
                    }
                }
                self.flush_expr_diags(binder.diags);
                if in_es.len() != args.len() - 1 {
                    continue;
                }
                let mut acc: Option<ValueId> = None;
                for ie in in_es {
                    let Some(iv) = self.eval_comb_expr(&arena, ie).and_then(|o| o.as_packed()) else { continue };
                    acc = Some(match acc {
                        None => iv,
                        Some(prev) => {
                            let op = match gate_kw {
                                "and" | "nand" => NetOp::And,
                                "or" | "nor" => NetOp::Or,
                                _ => NetOp::Xor,
                            };
                            self.builder.alloc_op(op, vec![prev, iv], 1, false, true, loc)
                        }
                    });
                }
                if let Some(mut v) = acc {
                    if matches!(gate_kw, "nand" | "nor" | "xnor") {
                        v = self.builder.alloc_op(NetOp::Not, vec![v], 1, false, true, loc);
                    }
                    self.drive_bound_lhs_continuous(
                        &arena,
                        out_e,
                        ObjVal::Packed(v),
                        DriverKind::ContinuousAssign,
                        loc,
                    );
                }
            }
        }
    }

    fn lower_net_alias(&mut self, inst_id: InstanceId, item: &sv_semantic::ElabItem) {
        let t = &self.trees[item.tree];
        let terms: Vec<NodeId> = t.children(item.node).filter(|&c| t.kind(c) != SyntaxKind::Attribute).collect();
        if terms.len() < 2 {
            self.builder.err(item.loc, "malformed 'alias' statement");
            return;
        }
        if terms.len() > 2 {
            self.builder.unsup(item.loc, "'alias' statement with more than 2 nets is not supported");
            return;
        }
        let mut arena = ExprArena::new();
        let mut binder =
            ExprBinder::new(self.trees, self.design, inst_id, &item.scope_prefix, &item.gen_consts, &mut arena);
        let a_opt = binder.bind_self(item.tree, terms[0]);
        let b_opt = binder.bind_self(item.tree, terms[1]);
        self.flush_expr_diags(binder.diags);
        let (Some(a_eid), Some(b_eid)) = (a_opt, b_opt) else {
            return;
        };
        let (Some(wa), Some(wb)) = (arena.get(a_eid).ty.bit_width(), arena.get(b_eid).ty.bit_width()) else {
            self.builder.unsup(item.loc, "'alias' target does not resolve to a packed net");
            return;
        };
        if wa != wb || wa == 0 {
            self.builder.err(item.loc, format!("'alias' nets must have equal non-zero width (got {wa} and {wb})"));
            return;
        }
        let res_a = self.resolve_const_lhs_slice(&arena, a_eid, item.loc);
        let res_b = self.resolve_const_lhs_slice(&arena, b_eid, item.loc);
        let (Some((ObjSlot::Packed(sidx_a), lo_a, bit_wa)), Some((ObjSlot::Packed(sidx_b), lo_b, bit_wb))) =
            (res_a, res_b)
        else {
            self.builder.unsup(item.loc, "'alias' expression cannot be resolved to a constant net slice");
            return;
        };
        if bit_wa != wa || bit_wb != wb || !self.slots[sidx_a].is_net || !self.slots[sidx_b].is_net {
            self.builder.unsup(item.loc, "'alias' requires net operands with constant slice bounds");
            return;
        }
        self.pending_aliases.push(PendingAlias { sidx_a, lo_a, sidx_b, lo_b, width: wa, loc: item.loc });
    }

    fn resolve_pending_aliases(&mut self) {
        let pending = std::mem::take(&mut self.pending_aliases);
        for pa in pending {
            let hi_a = pa.lo_a + pa.width - 1;
            let hi_b = pa.lo_b + pa.width - 1;
            if pa.sidx_a == pa.sidx_b && pa.lo_a <= hi_b && pa.lo_b <= hi_a {
                if pa.lo_a == pa.lo_b {
                    continue;
                }
                self.builder.unsup(pa.loc, "self-overlapping 'alias' on the same net is not supported");
                continue;
            }
            let a_driven = self.slots[pa.sidx_a].drivers.iter().any(|d| d.lo <= hi_a && pa.lo_a <= d.hi);
            let b_driven = self.slots[pa.sidx_b].drivers.iter().any(|d| d.lo <= hi_b && pa.lo_b <= d.hi);
            match (a_driven, b_driven) {
                (true, false) => {
                    let src_ph = self.slot_placeholders[pa.sidx_a];
                    let src_val = if pa.lo_a == 0 && pa.width == self.slots[pa.sidx_a].width {
                        src_ph
                    } else {
                        self.builder.alloc_op(
                            NetOp::Slice { hi: hi_a, lo: pa.lo_a },
                            vec![src_ph],
                            pa.width,
                            false,
                            self.slots[pa.sidx_a].four_state,
                            pa.loc,
                        )
                    };
                    self.slots[pa.sidx_b].add_driver(pa.lo_b, hi_b, src_val, DriverKind::ContinuousAssign, pa.loc);
                }
                (false, true) => {
                    let src_ph = self.slot_placeholders[pa.sidx_b];
                    let src_val = if pa.lo_b == 0 && pa.width == self.slots[pa.sidx_b].width {
                        src_ph
                    } else {
                        self.builder.alloc_op(
                            NetOp::Slice { hi: hi_b, lo: pa.lo_b },
                            vec![src_ph],
                            pa.width,
                            false,
                            self.slots[pa.sidx_b].four_state,
                            pa.loc,
                        )
                    };
                    self.slots[pa.sidx_a].add_driver(pa.lo_a, hi_a, src_val, DriverKind::ContinuousAssign, pa.loc);
                }
                _ => {
                    self.builder.unsup(
                        pa.loc,
                        "bidirectional or undriven 'alias' net statement is not supported in directional RTL netlist",
                    );
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn drive_lhs_expr_continuous(
        &mut self,
        inst_id: InstanceId,
        tree_idx: usize,
        lhs_node: NodeId,
        scope_prefix: &str,
        gen_consts: &[(String, CVal)],
        rhs_val: ObjVal,
        rhs_ty: &RtlType,
        kind: DriverKind,
        loc: Loc,
    ) {
        self.check_modport_lhs_legality(inst_id, tree_idx, lhs_node, scope_prefix, loc);
        let mut arena = ExprArena::new();
        let mut binder = ExprBinder::new(self.trees, self.design, inst_id, scope_prefix, gen_consts, &mut arena);
        let Some(lhs_eid) = binder.bind_self(tree_idx, lhs_node) else {
            self.flush_expr_diags(binder.diags);
            return;
        };
        self.flush_expr_diags(binder.diags);
        let lhs_ty = arena.get(lhs_eid).ty.clone();
        let coerced = self.coerce_obj_val(rhs_val, rhs_ty, &lhs_ty, loc);
        self.drive_bound_lhs_continuous(&arena, lhs_eid, coerced, kind, loc);
    }

    fn drive_bound_lhs_continuous(
        &mut self,
        arena: &ExprArena,
        lhs_eid: ExprId,
        rhs_val: ObjVal,
        kind: DriverKind,
        loc: Loc,
    ) {
        let lhs_expr = arena.get(lhs_eid);
        match &lhs_expr.kind {
            ExprKind::Symbol(sid) => {
                if let Some(slot) = self.sym_slots.get(sid).cloned() {
                    self.drive_obj_slot(&slot, rhs_val, kind, loc);
                }
            }
            ExprKind::Concat(parts) => {
                let Some(rv) = rhs_val.as_packed() else {
                    self.builder.err(loc, "concatenation LHS requires packed RHS value");
                    return;
                };
                let (total_w, _, four) = self.builder.node_shape(rv);
                let mut hi_cursor = total_w;
                for &part_eid in parts {
                    let pw = arena.get(part_eid).ty.bit_width().unwrap_or(1);
                    if hi_cursor < pw {
                        break;
                    }
                    let lo = hi_cursor - pw;
                    let slice =
                        self.builder.alloc_op(NetOp::Slice { hi: hi_cursor - 1, lo }, vec![rv], pw, false, four, loc);
                    self.drive_bound_lhs_continuous(arena, part_eid, ObjVal::Packed(slice), kind, loc);
                    hi_cursor = lo;
                }
            }
            _ => {
                // Constant bit-select / part-select / member chain on a signal (`assign w[3:0] = a;`, `assign s.field = b;`)!
                if let Some((slot, bit_lo, bit_w)) = self.resolve_const_lhs_slice(arena, lhs_eid, loc) {
                    if let ObjSlot::Packed(sidx) = slot {
                        if let Some(rv) = rhs_val.as_packed() {
                            let resized = self.builder.resize_val(rv, bit_w, false, false, loc);
                            self.slots[sidx].add_driver(bit_lo, bit_lo + bit_w - 1, resized, kind, loc);
                        }
                    } else if bit_lo == 0 {
                        self.drive_obj_slot(&slot, rhs_val, kind, loc);
                    }
                } else {
                    self.builder.err(loc, "continuous assignment LHS must have constant bit/element bounds");
                }
            }
        }
    }

    fn resolve_const_lhs_slice(&mut self, arena: &ExprArena, eid: ExprId, _loc: Loc) -> Option<(ObjSlot, u32, u32)> {
        let e = arena.get(eid);
        match &e.kind {
            ExprKind::Symbol(sid) => {
                let slot = self.sym_slots.get(sid)?.clone();
                let w = e.ty.bit_width().unwrap_or(0);
                Some((slot, 0, w))
            }
            ExprKind::Member { base, field, packed_slice } => {
                let (base_slot, base_lo, _) = self.resolve_const_lhs_slice(arena, *base, _loc)?;
                if let Some((f_lo, f_w)) = *packed_slice {
                    return Some((base_slot, base_lo + f_lo, f_w));
                }
                if let ObjSlot::Struct(fields) = base_slot {
                    let (_, fslot) = fields.into_iter().find(|(n, _)| n == field)?;
                    let w = e.ty.bit_width().unwrap_or(0);
                    return Some((fslot, 0, w));
                }
                None
            }
            ExprKind::BitSelect { base, index, bounds, elem_width } => {
                let (base_slot, base_lo, base_w) = self.resolve_const_lhs_slice(arena, *base, _loc)?;
                let icv = arena.get(*index).cval.as_ref()?;
                if !icv.known() {
                    return None;
                }
                let off = sv_semantic::expr::map_bit_offset(icv.i(), *bounds, *elem_width, base_w)?;
                Some((base_slot, base_lo + off, *elem_width))
            }
            ExprKind::PartSelect { base, msb, lsb, bounds, elem_width } => {
                let (base_slot, base_lo, base_w) = self.resolve_const_lhs_slice(arena, *base, _loc)?;
                let low_idx = if bounds.0 >= bounds.1 { (*msb).min(*lsb) } else { (*msb).max(*lsb) };
                let count = (((*msb - *lsb).unsigned_abs() as u32) + 1) * (*elem_width);
                let off = sv_semantic::expr::map_bit_offset(low_idx, *bounds, *elem_width, base_w)?;
                Some((base_slot, base_lo + off, count))
            }
            ExprKind::IndexedPartSelect { base, base_idx, width, ascending, bounds, elem_width } => {
                let (base_slot, base_lo, base_w) = self.resolve_const_lhs_slice(arena, *base, _loc)?;
                let icv = arena.get(*base_idx).cval.as_ref()?;
                if !icv.known() {
                    return None;
                }
                let span = (width.saturating_sub(1)) as i128;
                let (lo_sv, hi_sv) = if *ascending { (icv.i(), icv.i() + span) } else { (icv.i() - span, icv.i()) };
                let phys_idx = if bounds.0 >= bounds.1 { lo_sv } else { hi_sv };
                let off = sv_semantic::expr::map_bit_offset(phys_idx, *bounds, *elem_width, base_w)?;
                Some((base_slot, base_lo + off, (*width) * (*elem_width)))
            }
            ExprKind::ArrayIndex { base, index, dim } => {
                let (base_slot, _, _) = self.resolve_const_lhs_slice(arena, *base, _loc)?;
                let icv = arena.get(*index).cval.as_ref()?;
                if !icv.known() {
                    return None;
                }
                let min_b = dim.0.min(dim.1);
                let max_b = dim.0.max(dim.1);
                if icv.i() < min_b || icv.i() > max_b {
                    return None;
                }
                if let ObjSlot::Elems { elems, .. } = base_slot {
                    let eslot = elems.get((icv.i() - min_b) as usize)?.clone();
                    let w = e.ty.bit_width().unwrap_or(0);
                    return Some((eslot, 0, w));
                }
                None
            }
            _ => None,
        }
    }

    pub(crate) fn drive_obj_slot(&mut self, slot: &ObjSlot, val: ObjVal, kind: DriverKind, loc: Loc) {
        match (slot, val) {
            (ObjSlot::Packed(sidx), ObjVal::Packed(v)) => {
                let w = self.slots[*sidx].width;
                let resized = self.builder.resize_val(v, w, false, self.slots[*sidx].signed, loc);
                self.slots[*sidx].add_driver(0, w - 1, resized, kind, loc);
            }
            (ObjSlot::Struct(sfields), ObjVal::Struct(vfields)) => {
                for (fname, fslot) in sfields {
                    if let Some((_, fv)) = vfields.iter().find(|(n, _)| n == fname) {
                        self.drive_obj_slot(fslot, fv.clone(), kind, loc);
                    }
                }
            }
            (ObjSlot::Elems { elems: selems, .. }, ObjVal::Elems { elems: velems, .. }) => {
                for (es, ev) in selems.iter().zip(velems) {
                    self.drive_obj_slot(es, ev, kind, loc);
                }
            }
            _ => {
                self.builder.err(loc, "incompatible aggregate port/continuous connection");
            }
        }
    }

    pub(crate) fn read_slot_obj(&mut self, slot: &ObjSlot, loc: Loc) -> ObjVal {
        match slot {
            ObjSlot::Packed(sidx) => ObjVal::Packed(self.slot_placeholders[*sidx]),
            ObjSlot::Const(cv, four) => ObjVal::Packed(self.builder.alloc_const(cv.clone(), *four, loc)),
            ObjSlot::Struct(fields) => {
                ObjVal::Struct(fields.iter().map(|(n, fs)| (n.clone(), self.read_slot_obj(fs, loc))).collect())
            }
            ObjSlot::Elems { left, right, elems } => ObjVal::Elems {
                left: *left,
                right: *right,
                elems: elems.iter().map(|es| self.read_slot_obj(es, loc)).collect(),
            },
            ObjSlot::Memory(mid) => ObjVal::Memory(*mid),
        }
    }

    pub(crate) fn coerce_obj_val(&mut self, val: ObjVal, src_ty: &RtlType, dst_ty: &RtlType, loc: Loc) -> ObjVal {
        match (val, src_ty.integral_shape(), dst_ty.integral_shape()) {
            (ObjVal::Packed(v), Some((_, src_s, _)), Some((dst_w, dst_s, dst_4))) => {
                let resized = self.builder.resize_val(v, dst_w, src_s, dst_s, loc);
                let (_, _, cur_4) = self.builder.node_shape(resized);
                if cur_4 != dst_4 {
                    ObjVal::Packed(self.builder.alloc_op(
                        NetOp::CastFourState { four_state: dst_4 },
                        vec![resized],
                        dst_w,
                        dst_s,
                        dst_4,
                        loc,
                    ))
                } else {
                    ObjVal::Packed(resized)
                }
            }
            (other, _, _) => other,
        }
    }

    pub(crate) fn eval_comb_expr(&mut self, arena: &ExprArena, eid: ExprId) -> Option<ObjVal> {
        let empty_syms = FxMap::default();
        let empty_locals = FxMap::default();
        let sym_slots = self.sym_slots.clone();
        let placeholders = self.slot_placeholders.clone();
        let config = self.config;
        let mut builder = std::mem::replace(&mut self.builder, NetBuilder::new(String::new(), Vec::new()));
        let mut lowerer = ExprLowerer {
            arena,
            config,
            active_clock: None,
            sym_slots: &sym_slots,
            slot_placeholders: &placeholders,
            cur_syms: &empty_syms,
            cur_locals: &empty_locals,
            inliner: self,
        };
        let res = lowerer.lower_any(&mut builder, eid);
        self.builder = builder;
        res
    }

    pub(crate) fn check_modport_lhs_legality(
        &mut self,
        inst_id: InstanceId,
        tree_idx: usize,
        mut lhs_node: NodeId,
        scope_prefix: &str,
        loc: Loc,
    ) {
        let t = &self.trees[tree_idx];
        // Peel outer selects/members down to `MemberExpr(NameExpr(iface_port), signal)`.
        loop {
            match t.kind(lhs_node) {
                SyntaxKind::SelectExpr => {
                    if let Some(base) = t.children(lhs_node).find(|&c| t.kind(c) != SyntaxKind::Attribute) {
                        lhs_node = base;
                        continue;
                    }
                }
                SyntaxKind::MemberExpr => {
                    let Some(base) = t.children(lhs_node).find(|&c| t.kind(c) != SyntaxKind::Attribute) else { break };
                    if t.kind(base) == SyntaxKind::NameExpr {
                        let base_raw = t.bare_text(base);
                        let base_name = base_raw.trim();
                        if let Some(psid) = self.design.instance(inst_id).resolve_lexical(scope_prefix, base_name) {
                            if matches!(self.design.symbol(psid).ty, RtlType::Interface { .. }) {
                                if let Some((iface_id, Some(mp_name))) =
                                    resolve_interface_port(self.trees, self.design, inst_id, psid)
                                {
                                    let field_tok = t
                                        .node(lhs_node)
                                        .children
                                        .iter()
                                        .rev()
                                        .find_map(|c| match *c {
                                            Child::Token(i) if !t.tokens[i as usize].kind.is_trivia() => {
                                                Some(&*t.tokens[i as usize].text)
                                            }
                                            _ => None,
                                        })
                                        .unwrap_or("");
                                    let iface_inst = self.design.instance(iface_id);
                                    if let Some((_, mp_ports)) = iface_inst.modports.iter().find(|(n, _)| n == &mp_name)
                                    {
                                        if let Some((_, pdir)) = mp_ports.iter().find(|(sn, _)| sn == field_tok) {
                                            if *pdir == PortDir::Input {
                                                self.builder.err(
                                                    loc,
                                                    format!(
                                                        "cannot drive signal '{field_tok}' through modport '{mp_name}' (declared input)"
                                                    ),
                                                );
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        break;
                    }
                    lhs_node = base;
                    continue;
                }
                _ => break,
            }
            break;
        }
    }

    pub(crate) fn reject_tristate_z(&mut self, arena: &ExprArena, eid: ExprId) -> bool {
        let e = arena.get(eid);
        if let Some(cv) = &e.cval {
            for bit in 0..cv.w {
                if cv.bit_state(bit) == sv_semantic::BitState::Z {
                    self.builder.unsup(e.loc, "tri-state high-impedance ('z') driver is not supported in RTL netlist");
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn collect_scope_gen_consts(&self, inst_id: InstanceId, scope_prefix: &str) -> Vec<(String, CVal)> {
        let inst = self.design.instance(inst_id);
        let mut scoped: Vec<(usize, String, CVal)> = Vec::new();
        for &sid in &inst.symbols {
            let sym = self.design.symbol(sid);
            if sym.kind == SymbolKind::Genvar {
                if let Some(cv) = &sym.cval {
                    if sym.rel_path == sym.name
                        || scope_prefix == sym.init_scope_prefix
                        || scope_prefix.starts_with(&format!("{}.", sym.init_scope_prefix))
                    {
                        scoped.push((sym.init_scope_prefix.len(), sym.name.clone(), cv.clone()));
                    }
                }
            }
        }
        scoped.sort_by_key(|a| std::cmp::Reverse(a.0));
        scoped.into_iter().map(|(_, name, cv)| (name, cv)).collect()
    }

    pub(crate) fn flush_expr_diags(&mut self, diags: Vec<sv_semantic::ExprDiag>) {
        for d in diags {
            if d.unsupported {
                self.builder.unsup(d.loc, d.message);
            } else {
                self.builder.err(d.loc, d.message);
            }
        }
    }

    fn collect_sva_recursive(
        &mut self,
        inst_id: InstanceId,
        sva_arena: &mut ExprArena,
        properties: &mut Vec<BoundProperty>,
    ) {
        let mut binder = SvaBinder::new(self.trees, self.design, sva_arena);
        let bound = binder.bind_instance_properties(inst_id);
        let diags = binder.diags;
        self.flush_expr_diags(diags);
        properties.extend(bound);

        let children = self.design.instance(inst_id).children.clone();
        for cid in children {
            self.collect_sva_recursive(cid, sva_arena, properties);
        }
    }

    fn lower_sva_properties(&mut self, sva_arena: &ExprArena, properties: &[BoundProperty]) -> Vec<Option<ValueId>> {
        let mut out = vec![None; sva_arena.exprs.len()];
        let empty_syms = FxMap::default();
        let empty_locals = FxMap::default();
        let sym_slots = self.sym_slots.clone();
        let placeholders = self.slot_placeholders.clone();
        let config = self.config;
        let mut builder = std::mem::replace(&mut self.builder, NetBuilder::new(String::new(), Vec::new()));

        for prop in properties {
            lower_bound_property_exprs(
                sva_arena,
                prop,
                config,
                &sym_slots,
                &placeholders,
                &empty_syms,
                &empty_locals,
                self,
                &mut builder,
                &mut out,
            );
        }

        self.builder = builder;
        out
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_bound_property_exprs<I: FuncInliner>(
    arena: &ExprArena,
    prop: &BoundProperty,
    config: &LowerConfig,
    sym_slots: &FxMap<SymbolId, ObjSlot>,
    placeholders: &[ValueId],
    empty_syms: &FxMap<SymbolId, ObjVal>,
    empty_locals: &FxMap<sv_semantic::LocalId, ObjVal>,
    inliner: &mut I,
    builder: &mut NetBuilder,
    out: &mut [Option<ValueId>],
) {
    let mut lower_one =
        |eid: ExprId, is_clk_sig: bool, active_clock: Option<ClockSpec>, builder: &mut NetBuilder| -> Option<ValueId> {
            let idx = eid.0 as usize;
            if let Some(existing) = out.get(idx).and_then(|v| *v) {
                return Some(existing);
            }
            let loc = arena.get(eid).loc;
            let mut lowerer = ExprLowerer {
                arena,
                config,
                active_clock,
                sym_slots,
                slot_placeholders: placeholders,
                cur_syms: empty_syms,
                cur_locals: empty_locals,
                inliner,
            };
            let raw = lowerer.lower_packed(builder, eid)?;
            let val = if is_clk_sig { extract_lsb_bit0(builder, raw, loc) } else { raw };
            if let Some(slot) = out.get_mut(idx) {
                *slot = Some(val);
            }
            Some(val)
        };

    fn resolve_sva_clock<F>(clock: &SvaClock, lower_one: &mut F, builder: &mut NetBuilder) -> Option<ClockSpec>
    where
        F: FnMut(ExprId, bool, Option<ClockSpec>, &mut NetBuilder) -> Option<ValueId>,
    {
        let sig_val = lower_one(clock.signal, true, None, builder);
        let clk_spec = sig_val.and_then(|signal| match clock.edge {
            SvaEdge::Posedge => Some(ClockSpec { signal, edge: Edge::Posedge }),
            SvaEdge::Negedge => Some(ClockSpec { signal, edge: Edge::Negedge }),
            SvaEdge::AnyEdge => None,
        });
        if let Some(iff_e) = clock.iff {
            let _ = lower_one(iff_e, false, clk_spec, builder);
        }
        clk_spec
    }

    fn walk_seq<F>(seq: &SeqExpr, clk: Option<ClockSpec>, lower_one: &mut F, builder: &mut NetBuilder)
    where
        F: FnMut(ExprId, bool, Option<ClockSpec>, &mut NetBuilder) -> Option<ValueId>,
    {
        match seq {
            SeqExpr::Bool(e) => {
                let _ = lower_one(*e, false, clk, builder);
            }
            SeqExpr::Delay { lhs, rhs, .. } => {
                if let Some(l) = lhs {
                    walk_seq(l, clk, lower_one, builder);
                }
                walk_seq(rhs, clk, lower_one, builder);
            }
            SeqExpr::Fusion(a, b)
            | SeqExpr::And(a, b)
            | SeqExpr::Or(a, b)
            | SeqExpr::Intersect(a, b)
            | SeqExpr::Within(a, b) => {
                walk_seq(a, clk, lower_one, builder);
                walk_seq(b, clk, lower_one, builder);
            }
            SeqExpr::Repeat { seq: inner, .. } | SeqExpr::FirstMatch(inner) => {
                walk_seq(inner, clk, lower_one, builder);
            }
            SeqExpr::Throughout { cond, seq: inner } => {
                let _ = lower_one(*cond, false, clk, builder);
                walk_seq(inner, clk, lower_one, builder);
            }
            SeqExpr::Clocked { clock, seq: inner } => {
                let inner_clk = resolve_sva_clock(clock, lower_one, builder).or(clk);
                walk_seq(inner, inner_clk, lower_one, builder);
            }
        }
    }

    fn walk_prop<F>(p: &PropExpr, clk: Option<ClockSpec>, lower_one: &mut F, builder: &mut NetBuilder)
    where
        F: FnMut(ExprId, bool, Option<ClockSpec>, &mut NetBuilder) -> Option<ValueId>,
    {
        match p {
            PropExpr::Seq(s) | PropExpr::Strong(s) | PropExpr::Weak(s) => walk_seq(s, clk, lower_one, builder),
            PropExpr::Not(inner)
            | PropExpr::Nexttime { prop: inner, .. }
            | PropExpr::Always { prop: inner, .. }
            | PropExpr::Eventually { prop: inner, .. } => walk_prop(inner, clk, lower_one, builder),
            PropExpr::And(a, b)
            | PropExpr::Or(a, b)
            | PropExpr::Implies(a, b)
            | PropExpr::Iff(a, b)
            | PropExpr::Until { lhs: a, rhs: b, .. } => {
                walk_prop(a, clk, lower_one, builder);
                walk_prop(b, clk, lower_one, builder);
            }
            PropExpr::OverlappedImplication { lhs, rhs }
            | PropExpr::NonOverlappedImplication { lhs, rhs }
            | PropExpr::FollowedBy { lhs, rhs, .. } => {
                walk_seq(lhs, clk, lower_one, builder);
                walk_prop(rhs, clk, lower_one, builder);
            }
            PropExpr::IfElse { cond, then_prop, else_prop } => {
                let _ = lower_one(*cond, false, clk, builder);
                walk_prop(then_prop, clk, lower_one, builder);
                if let Some(ep) = else_prop {
                    walk_prop(ep, clk, lower_one, builder);
                }
            }
            PropExpr::AcceptOn { cond, prop: inner, .. } => {
                let _ = lower_one(*cond, false, clk, builder);
                walk_prop(inner, clk, lower_one, builder);
            }
            PropExpr::Clocked { clock, prop: inner } => {
                let inner_clk = resolve_sva_clock(clock, lower_one, builder).or(clk);
                walk_prop(inner, inner_clk, lower_one, builder);
            }
        }
    }

    let top_clk = prop.clock.as_ref().and_then(|c| resolve_sva_clock(c, &mut lower_one, builder));
    if let Some(dis) = prop.disable_iff {
        let _ = lower_one(dis, false, top_clk, builder);
    }
    walk_prop(&prop.expr, top_clk, &mut lower_one, builder);
}

struct ExternalPropInliner;

impl FuncInliner for ExternalPropInliner {
    fn inline_func_call(
        &mut self,
        builder: &mut NetBuilder,
        _callee: (usize, NodeId),
        _callee_inst: Option<InstanceId>,
        _callee_scope_prefix: &str,
        _name: &str,
        args: &[ObjVal],
        _ret_ty: &RtlType,
        loc: Loc,
    ) -> Option<ObjVal> {
        let _ = args;
        builder.unsup(loc, "non-constant user function call in external SVA property is not supported");
        None
    }
}

pub(crate) fn lower_external_property_on_netlist(
    nl: &mut Netlist,
    design: &ElaboratedDesign,
    arena: &ExprArena,
    prop: &BoundProperty,
    allow_four_state_ops: bool,
) -> Result<Vec<(ExprId, ValueId)>, Vec<NetlistDiag>> {
    let mut builder = NetBuilder::from_netlist(nl);
    let max_file = arena
        .exprs
        .iter()
        .map(|e| e.loc.file as usize)
        .chain(std::iter::once(prop.loc.file as usize))
        .max()
        .unwrap_or(0);
    while builder.files.len() <= max_file {
        let idx = builder.files.len();
        let name = design.files.get(idx).cloned().unwrap_or_else(|| "<property>".into());
        builder.files.push(name);
    }

    let trace_map: FxMap<&str, ValueId> = nl.traces.iter().map(|tr| (tr.name.as_str(), tr.value)).collect();
    let mem_map: FxMap<&str, MemId> = nl.memories.iter().map(|m| (m.name.as_str(), m.id)).collect();
    let mut sym_slots: FxMap<SymbolId, ObjSlot> = FxMap::default();
    let mut placeholders: Vec<ValueId> = Vec::new();

    fn rebuild_agg_slot(
        full_path: &str,
        ty: &RtlType,
        trace_map: &FxMap<&str, ValueId>,
        placeholders: &mut Vec<ValueId>,
    ) -> Option<ObjSlot> {
        if ty.integral_shape().is_some() {
            let vid = *trace_map.get(full_path)?;
            let idx = placeholders.len();
            placeholders.push(vid);
            return Some(ObjSlot::Packed(idx));
        }
        match ty {
            RtlType::UnpackedStruct { is_union: false, fields, .. } => {
                let mut out = Vec::with_capacity(fields.len());
                for (fname, fty) in fields {
                    let sub_path = format!("{full_path}.{fname}");
                    out.push((fname.clone(), rebuild_agg_slot(&sub_path, fty, trace_map, placeholders)?));
                }
                Some(ObjSlot::Struct(out))
            }
            RtlType::UnpackedArray { elem, dims } if !dims.is_empty() => {
                let (left, right) = dims[0];
                let min_b = left.min(right);
                let max_b = left.max(right);
                let sub_ty = if dims.len() > 1 {
                    RtlType::UnpackedArray { elem: elem.clone(), dims: dims[1..].to_vec() }
                } else {
                    (**elem).clone()
                };
                let mut elems = Vec::new();
                for sv_idx in min_b..=max_b {
                    let sub_path = format!("{full_path}[{sv_idx}]");
                    elems.push(rebuild_agg_slot(&sub_path, &sub_ty, trace_map, placeholders)?);
                }
                Some(ObjSlot::Elems { left, right, elems })
            }
            _ => None,
        }
    }

    for (idx, sym) in design.symbols.iter().enumerate() {
        let sid = SymbolId(idx as u32);
        if let Some(vid) = nl.symbol_value(sid) {
            let sidx = placeholders.len();
            placeholders.push(vid);
            sym_slots.insert(sid, ObjSlot::Packed(sidx));
        } else if matches!(sym.kind, SymbolKind::Param { .. } | SymbolKind::Genvar) {
            if let Some(cv) = &sym.cval {
                sym_slots.insert(sid, ObjSlot::Const(cv.clone(), sym.ty.is_four_state()));
            }
        } else if let Some(&mid) = mem_map.get(sym.full_path.as_str()) {
            sym_slots.insert(sid, ObjSlot::Memory(mid));
        } else if let Some(slot) = rebuild_agg_slot(&sym.full_path, &sym.ty, &trace_map, &mut placeholders) {
            sym_slots.insert(sid, slot);
        }
    }

    let config = LowerConfig { allow_four_state_ops, ..LowerConfig::default() };
    let empty_syms = FxMap::default();
    let empty_locals = FxMap::default();
    let mut inliner = ExternalPropInliner;
    let mut lowered_map: Vec<Option<ValueId>> = vec![None; arena.exprs.len()];

    lower_bound_property_exprs(
        arena,
        prop,
        &config,
        &sym_slots,
        &placeholders,
        &empty_syms,
        &empty_locals,
        &mut inliner,
        &mut builder,
        &mut lowered_map,
    );

    if !builder.diags.is_empty() {
        return Err(builder.diags);
    }

    let mut pairs: Vec<(ExprId, ValueId)> =
        lowered_map.into_iter().enumerate().filter_map(|(i, opt)| opt.map(|vid| (ExprId(i as u32), vid))).collect();

    let old_val_len = nl.values.len();
    let old_samp_len = nl.sampled.len();
    if builder.nodes.len() > old_val_len || builder.sampled.len() > old_samp_len {
        let resolve_alias = |mut v: ValueId| -> ValueId {
            while let BuildKind::Alias(next) = builder.nodes[v.0 as usize].kind {
                v = next;
            }
            v
        };
        for i in old_val_len..builder.nodes.len() {
            let bn = &builder.nodes[i];
            let kind = match &bn.kind {
                BuildKind::Placeholder | BuildKind::Alias(_) => ValueKind::Const(CVal::new(bn.width, bn.signed, 0)),
                BuildKind::Input(pi) => ValueKind::Input(*pi),
                BuildKind::Const(cv) => ValueKind::Const(cv.clone()),
                BuildKind::StateOut(sid) => ValueKind::StateOut(*sid),
                BuildKind::Op { op, args } => {
                    ValueKind::Op { op: op.clone(), args: args.iter().map(|&a| resolve_alias(a)).collect() }
                }
                BuildKind::MemRead { mem, addr } => ValueKind::MemRead { mem: *mem, addr: resolve_alias(*addr) },
                BuildKind::Sampled(si) => ValueKind::Sampled(*si),
                BuildKind::Undriven(ui) => ValueKind::Undriven(*ui),
            };
            nl.values.push(ValueNode {
                id: ValueId(i as u32),
                width: bn.width,
                signed: bn.signed,
                four_state: bn.four_state,
                kind,
                name: bn.name.clone(),
                loc: bn.loc,
            });
        }
        for sn in builder.sampled.into_iter().skip(old_samp_len) {
            let mut sn = sn;
            sn.expr = resolve_alias(sn.expr);
            if let Some(clk) = &mut sn.clock {
                clk.signal = resolve_alias(clk.signal);
            }
            if let Some(g) = &mut sn.gate {
                *g = resolve_alias(*g);
            }
            sn.out = resolve_alias(sn.out);
            nl.sampled.push(sn);
        }
        for (_, vid) in &mut pairs {
            *vid = resolve_alias(*vid);
        }

        let orig_prop_len = nl.prop_expr_values.len();
        for &(_, vid) in &pairs {
            nl.prop_expr_values.push(Some(vid));
        }
        nl.toposort();
        for (idx, (_, vid)) in pairs.iter_mut().enumerate() {
            if let Some(Some(remapped)) = nl.prop_expr_values.get(orig_prop_len + idx) {
                *vid = *remapped;
            }
        }
        nl.prop_expr_values.truncate(orig_prop_len);
    }

    Ok(pairs)
}

pub(crate) fn validate_rtl_type(builder: &mut NetBuilder, full_path: &str, ty: &RtlType, loc: Loc) -> bool {
    match ty {
        RtlType::NonSynthesizable(reason) => {
            builder.err(loc, format!("'{full_path}' has non-synthesizable type '{reason}'"));
            false
        }
        RtlType::Event => {
            builder.unsup(loc, format!("named event '{full_path}' is not synthesizable in RTL"));
            false
        }
        RtlType::Void => {
            builder.unsup(loc, format!("void signal '{full_path}' is not supported in RTL"));
            false
        }
        RtlType::Integral { width, .. } | RtlType::Enum { width, .. } => {
            if *width == 0 {
                builder.unsup(loc, format!("zero-width packed type on '{full_path}' is not supported"));
                return false;
            }
            true
        }
        RtlType::PackedArray { elem, width, .. } => {
            if *width == 0 {
                builder.unsup(loc, format!("zero-width packed array on '{full_path}' is not supported"));
                return false;
            }
            validate_rtl_type(builder, full_path, elem, loc)
        }
        RtlType::PackedStruct { width, fields, .. } | RtlType::PackedUnion { width, fields, .. } => {
            if *width == 0 {
                builder.unsup(loc, format!("zero-width packed struct/union on '{full_path}' is not supported"));
                return false;
            }
            for pf in fields {
                let sub = format!("{full_path}.{}", pf.name);
                if !validate_rtl_type(builder, &sub, &pf.ty, loc) {
                    return false;
                }
            }
            true
        }
        RtlType::UnpackedStruct { is_union, fields, .. } => {
            if *is_union {
                builder.unsup(loc, format!("unpacked union '{full_path}' is not synthesizable; use 'union packed'"));
                return false;
            }
            for (fname, fty) in fields {
                let sub = format!("{full_path}.{fname}");
                if !validate_rtl_type(builder, &sub, fty, loc) {
                    return false;
                }
            }
            true
        }
        RtlType::UnpackedArray { elem, dims } => {
            if dims.is_empty() {
                builder.unsup(loc, format!("empty unpacked array dimension on '{full_path}' is not supported"));
                return false;
            }
            validate_rtl_type(builder, full_path, elem, loc)
        }
        RtlType::Interface { .. } => {
            builder.unsup(loc, format!("interface handle '{full_path}' cannot be allocated as a value slot"));
            false
        }
    }
}

pub(crate) fn total_unpacked_elems(ty: &RtlType) -> Option<u64> {
    match ty {
        RtlType::UnpackedArray { elem, dims } => {
            let mut acc: u64 = 1;
            for &(l, r) in dims {
                let span = (l - r).unsigned_abs().checked_add(1)?;
                let len = u64::try_from(span).ok()?;
                acc = acc.checked_mul(len)?;
            }
            acc.checked_mul(total_unpacked_elems(elem)?)
        }
        RtlType::UnpackedStruct { fields, .. } => {
            let mut sum: u64 = 0;
            for (_, fty) in fields {
                sum = sum.checked_add(total_unpacked_elems(fty)?)?;
            }
            Some(sum.max(1))
        }
        _ => Some(1),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProcScanMode {
    Initial,
    Comb,
    Clocked((InstanceId, usize)),
}

fn prescan_eligible_memories(trees: &[Tree], design: &ElaboratedDesign, top_id: InstanceId) -> FxSet<SymbolId> {
    let mut candidates: FxSet<SymbolId> = FxSet::default();
    let mut all_insts: Vec<InstanceId> = Vec::new();
    fn gather_insts(
        design: &ElaboratedDesign,
        id: InstanceId,
        insts: &mut Vec<InstanceId>,
        cands: &mut FxSet<SymbolId>,
    ) {
        insts.push(id);
        let inst = design.instance(id);
        for &sid in &inst.symbols {
            let sym = design.symbol(sid);
            if sym.kind == SymbolKind::Var {
                if let RtlType::UnpackedArray { elem, dims } = &sym.ty {
                    if dims.len() == 1 && elem.integral_shape().is_some_and(|(w, _, _)| w > 0) {
                        cands.insert(sid);
                    }
                }
            }
        }
        for &cid in &inst.children {
            gather_insts(design, cid, insts, cands);
        }
    }
    gather_insts(design, top_id, &mut all_insts, &mut candidates);
    if candidates.is_empty() {
        return candidates;
    }

    let mut disqualified: FxSet<SymbolId> = FxSet::default();
    let mut clocked_writers: FxMap<SymbolId, FxSet<(InstanceId, usize)>> = FxMap::default();

    let mark_aggregate_sym_uses = |arena: &ExprArena, disq: &mut FxSet<SymbolId>| {
        let mut indexed_bases: FxSet<u32> = FxSet::default();
        for e in &arena.exprs {
            if let ExprKind::ArrayIndex { base, .. } = &e.kind {
                if matches!(arena.get(*base).kind, ExprKind::Symbol(_)) {
                    indexed_bases.insert(base.0);
                }
            }
        }
        for (idx, e) in arena.exprs.iter().enumerate() {
            if let ExprKind::Symbol(sid) = e.kind {
                if candidates.contains(&sid) && !indexed_bases.contains(&(idx as u32)) {
                    disq.insert(sid);
                }
            }
        }
    };

    fn mark_lhs_root_syms(arena: &ExprArena, eid: ExprId, cands: &FxSet<SymbolId>, disq: &mut FxSet<SymbolId>) {
        match &arena.get(eid).kind {
            ExprKind::Symbol(sid) => {
                if cands.contains(sid) {
                    disq.insert(*sid);
                }
            }
            ExprKind::Concat(parts) => {
                for &p in parts {
                    mark_lhs_root_syms(arena, p, cands, disq);
                }
            }
            ExprKind::Member { base, .. }
            | ExprKind::BitSelect { base, .. }
            | ExprKind::PartSelect { base, .. }
            | ExprKind::IndexedPartSelect { base, .. }
            | ExprKind::ArrayIndex { base, .. } => {
                mark_lhs_root_syms(arena, *base, cands, disq);
            }
            _ => {}
        }
    }

    let scope_gen_consts = |inst_id: InstanceId, scope_prefix: &str| -> Vec<(String, CVal)> {
        let inst = design.instance(inst_id);
        let mut scoped: Vec<(usize, String, CVal)> = Vec::new();
        for &sid in &inst.symbols {
            let sym = design.symbol(sid);
            if sym.kind == SymbolKind::Genvar {
                if let Some(cv) = &sym.cval {
                    if sym.rel_path == sym.name
                        || scope_prefix == sym.init_scope_prefix
                        || scope_prefix.starts_with(&format!("{}.", sym.init_scope_prefix))
                    {
                        scoped.push((sym.init_scope_prefix.len(), sym.name.clone(), cv.clone()));
                    }
                }
            }
        }
        scoped.sort_by_key(|a| std::cmp::Reverse(a.0));
        scoped.into_iter().map(|(_, n, cv)| (n, cv)).collect()
    };

    for &inst_id in &all_insts {
        let inst = design.instance(inst_id);

        // 1. Net declaration continuous assignments.
        for &sid in &inst.symbols {
            let sym = design.symbol(sid);
            if sym.is_net_with_init() {
                if let Some((tr, nd)) = sym.init_node {
                    let gcs = scope_gen_consts(inst_id, &sym.init_scope_prefix);
                    let mut arena = ExprArena::new();
                    let mut binder = ExprBinder::new(trees, design, inst_id, &sym.init_scope_prefix, &gcs, &mut arena);
                    if binder.bind_assigned(tr, nd, &sym.ty).is_some() {
                        mark_aggregate_sym_uses(&arena, &mut disqualified);
                    }
                }
            } else if let Some((tr, nd)) = sym.init_node {
                let gcs = scope_gen_consts(inst_id, &sym.init_scope_prefix);
                let mut arena = ExprArena::new();
                let mut binder = ExprBinder::new(trees, design, inst_id, &sym.init_scope_prefix, &gcs, &mut arena);
                if binder.bind_assigned(tr, nd, &sym.ty).is_some() {
                    mark_aggregate_sym_uses(&arena, &mut disqualified);
                }
            }
        }

        // 2. Child port connections.
        for &cid in &inst.children {
            let child = design.instance(cid);
            for (port_idx, &port_sid) in child.ports.iter().enumerate() {
                let port_sym = design.symbol(port_sid);
                if matches!(port_sym.ty, RtlType::Interface { .. }) {
                    continue;
                }
                let SymbolKind::Port { dir, .. } = port_sym.kind else { continue };
                let conn_opt = child
                    .port_conns
                    .iter()
                    .find(|c| c.port_name.as_deref() == Some(port_sym.name.as_str()))
                    .or_else(|| {
                        child
                            .port_conns
                            .iter()
                            .find(|c| c.port_name.is_none() && !c.wildcard && c.arg_index == port_idx)
                    })
                    .or_else(|| child.port_conns.iter().find(|c| c.wildcard));
                if let Some(conn) = conn_opt {
                    if conn.implicit_named || conn.wildcard {
                        if let Some(psid) = inst.resolve_lexical(&conn.scope_prefix, &port_sym.name) {
                            if candidates.contains(&psid) {
                                disqualified.insert(psid);
                            }
                        }
                    } else if let Some((tr, nd)) = conn.expr {
                        let mut arena = ExprArena::new();
                        let mut binder =
                            ExprBinder::new(trees, design, inst_id, &conn.scope_prefix, &conn.gen_consts, &mut arena);
                        if let Some(eid) = binder.bind_self(tr, nd) {
                            mark_aggregate_sym_uses(&arena, &mut disqualified);
                            if dir != PortDir::Input {
                                mark_lhs_root_syms(&arena, eid, &candidates, &mut disqualified);
                            }
                        }
                    }
                }
            }
        }

        // 3. Concurrent SVA properties.
        {
            let mut sva_arena = ExprArena::new();
            let mut sva_binder = SvaBinder::new(trees, design, &mut sva_arena);
            let _ = sva_binder.bind_instance_properties(inst_id);
            mark_aggregate_sym_uses(&sva_arena, &mut disqualified);
        }

        // 4. Active items (`assign`, gates, `alias`, immediate assertions, procedural blocks).
        for (item_idx, item) in inst.items.iter().enumerate() {
            let t = &trees[item.tree];
            match item.kind {
                ElabItemKind::ContinuousAssign => {
                    for assign_expr in t.children(item.node).filter(|&c| t.kind(c) == SyntaxKind::BinaryExpr) {
                        let kids: Vec<NodeId> =
                            t.children(assign_expr).filter(|&c| t.kind(c) != SyntaxKind::Attribute).collect();
                        if kids.len() != 2 {
                            continue;
                        }
                        let mut arena = ExprArena::new();
                        let le_opt = {
                            let mut binder = ExprBinder::new(
                                trees,
                                design,
                                inst_id,
                                &item.scope_prefix,
                                &item.gen_consts,
                                &mut arena,
                            );
                            let le = binder.bind_self(item.tree, kids[0]);
                            let _ = binder.bind_self(item.tree, kids[1]);
                            le
                        };
                        if let Some(le) = le_opt {
                            mark_lhs_root_syms(&arena, le, &candidates, &mut disqualified);
                        }
                        mark_aggregate_sym_uses(&arena, &mut disqualified);
                    }
                }
                ElabItemKind::GateInst => {
                    let gate_kw = own_first_tok(t, item.node).unwrap_or("");
                    let arg_lists: Vec<NodeId> = subtree_preorder(t, item.node)
                        .into_iter()
                        .filter(|&n| t.kind(n) == SyntaxKind::ArgList)
                        .collect();
                    for al in arg_lists {
                        let args: Vec<NodeId> = t
                            .children(al)
                            .filter(|&c| t.kind(c) != SyntaxKind::Attribute)
                            .filter_map(|c| {
                                if t.kind(c) == SyntaxKind::Arg {
                                    t.children(c).find(|&k| t.kind(k) != SyntaxKind::Attribute)
                                } else {
                                    Some(c)
                                }
                            })
                            .collect();
                        if args.len() < 2 {
                            continue;
                        }
                        let mut arena = ExprArena::new();
                        let out_eids = {
                            let mut binder = ExprBinder::new(
                                trees,
                                design,
                                inst_id,
                                &item.scope_prefix,
                                &item.gen_consts,
                                &mut arena,
                            );
                            let out_slice =
                                if matches!(gate_kw, "buf" | "not") { &args[..args.len() - 1] } else { &args[..1] };
                            let mut outs = Vec::new();
                            for &on in out_slice {
                                if let Some(oe) = binder.bind_self(item.tree, on) {
                                    outs.push(oe);
                                }
                            }
                            for &an in &args {
                                let _ = binder.bind_self(item.tree, an);
                            }
                            outs
                        };
                        for oe in out_eids {
                            mark_lhs_root_syms(&arena, oe, &candidates, &mut disqualified);
                        }
                        mark_aggregate_sym_uses(&arena, &mut disqualified);
                    }
                }
                ElabItemKind::NetAlias => {
                    let mut arena = ExprArena::new();
                    let term_eids = {
                        let mut binder =
                            ExprBinder::new(trees, design, inst_id, &item.scope_prefix, &item.gen_consts, &mut arena);
                        let mut outs = Vec::new();
                        for term in t.children(item.node).filter(|&c| t.kind(c) != SyntaxKind::Attribute) {
                            if let Some(te) = binder.bind_self(item.tree, term) {
                                outs.push(te);
                            }
                        }
                        outs
                    };
                    for te in term_eids {
                        mark_lhs_root_syms(&arena, te, &candidates, &mut disqualified);
                    }
                    mark_aggregate_sym_uses(&arena, &mut disqualified);
                }
                ElabItemKind::AssertStmt => {
                    let mut arena = ExprArena::new();
                    let mut binder =
                        ExprBinder::new(trees, design, inst_id, &item.scope_prefix, &item.gen_consts, &mut arena);
                    for c in t.children(item.node) {
                        if t.kind(c) == SyntaxKind::ParenExpr {
                            let _ = binder.bind_self(item.tree, c);
                        }
                    }
                    mark_aggregate_sym_uses(&arena, &mut disqualified);
                }
                ElabItemKind::ProceduralBlock => {
                    let kw = own_first_tok(t, item.node).unwrap_or("");
                    let Some(body) = t.children(item.node).find(|&c| t.kind(c) != SyntaxKind::Attribute) else {
                        continue;
                    };
                    let mode = if kw == "initial" {
                        ProcScanMode::Initial
                    } else if kw == "always_ff" || (kw == "always" && is_edge_timed_stmt(t, body)) {
                        ProcScanMode::Clocked((inst_id, item_idx))
                    } else {
                        ProcScanMode::Comb
                    };
                    let mut locals = FxSet::default();
                    let mut visited_subs = FxSet::default();
                    prescan_proc_node(
                        trees,
                        design,
                        inst_id,
                        &item.scope_prefix,
                        item.tree,
                        body,
                        None,
                        mode,
                        &mut locals,
                        &mut visited_subs,
                        &candidates,
                        &mut disqualified,
                        &mut clocked_writers,
                    );
                }
                _ => {}
            }
        }
    }

    candidates
        .into_iter()
        .filter(|s| !disqualified.contains(s) && clocked_writers.get(s).map_or(0, |w| w.len()) <= 1)
        .collect()
}

fn is_edge_timed_stmt(t: &Tree, stmt: NodeId) -> bool {
    if t.kind(stmt) != SyntaxKind::TimedStmt {
        return false;
    }
    let Some(tc) = t.child(stmt, SyntaxKind::TimingControl) else {
        return false;
    };
    subtree_preorder(t, tc)
        .into_iter()
        .any(|n| own_first_tok(t, n).is_some_and(|tk| matches!(tk, "posedge" | "negedge")))
}

fn resolve_cst_ref_sym(
    trees: &[Tree],
    design: &ElaboratedDesign,
    inst_id: InstanceId,
    scope_prefix: &str,
    locals: &FxSet<String>,
    t: &Tree,
    node: NodeId,
) -> Option<SymbolId> {
    match t.kind(node) {
        SyntaxKind::NameExpr => {
            let raw = t.bare_text(node);
            let name = raw.trim().strip_prefix('\\').unwrap_or(raw.trim());
            if locals.contains(name) {
                return None;
            }
            design.instance(inst_id).resolve_lexical(scope_prefix, name)
        }
        SyntaxKind::MemberExpr => {
            let raw = t.bare_text(node);
            let path: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
            let first = path.split('.').next().unwrap_or("");
            if locals.contains(first) {
                return None;
            }
            if let Some(psid) = design.instance(inst_id).resolve_lexical(scope_prefix, first) {
                if matches!(design.symbol(psid).ty, RtlType::Interface { .. }) {
                    if let Some((iface_id, _)) = resolve_interface_port(trees, design, inst_id, psid) {
                        let rest = path.strip_prefix(first)?.strip_prefix('.')?;
                        return design.resolve_hierarchical(iface_id, "", rest);
                    }
                }
            }
            design.resolve_hierarchical(inst_id, scope_prefix, &path)
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn record_proc_lhs_write(
    trees: &[Tree],
    design: &ElaboratedDesign,
    inst_id: InstanceId,
    scope_prefix: &str,
    locals: &FxSet<String>,
    t: &Tree,
    lhs: NodeId,
    indexed: bool,
    mode: ProcScanMode,
    candidates: &FxSet<SymbolId>,
    disqualified: &mut FxSet<SymbolId>,
    clocked_writers: &mut FxMap<SymbolId, FxSet<(InstanceId, usize)>>,
) {
    match t.kind(lhs) {
        SyntaxKind::ParenExpr => {
            for c in t.children(lhs).filter(|&k| t.kind(k) != SyntaxKind::Attribute) {
                record_proc_lhs_write(
                    trees,
                    design,
                    inst_id,
                    scope_prefix,
                    locals,
                    t,
                    c,
                    indexed,
                    mode,
                    candidates,
                    disqualified,
                    clocked_writers,
                );
            }
        }
        SyntaxKind::ConcatExpr => {
            for c in t.children(lhs).filter(|&k| t.kind(k) != SyntaxKind::Attribute) {
                record_proc_lhs_write(
                    trees,
                    design,
                    inst_id,
                    scope_prefix,
                    locals,
                    t,
                    c,
                    false,
                    mode,
                    candidates,
                    disqualified,
                    clocked_writers,
                );
            }
        }
        SyntaxKind::SelectExpr => {
            if let Some(base) = t.children(lhs).find(|&k| t.kind(k) != SyntaxKind::Attribute) {
                record_proc_lhs_write(
                    trees,
                    design,
                    inst_id,
                    scope_prefix,
                    locals,
                    t,
                    base,
                    true,
                    mode,
                    candidates,
                    disqualified,
                    clocked_writers,
                );
            }
        }
        SyntaxKind::NameExpr | SyntaxKind::MemberExpr => {
            if let Some(sid) = resolve_cst_ref_sym(trees, design, inst_id, scope_prefix, locals, t, lhs) {
                if candidates.contains(&sid) {
                    if !indexed {
                        disqualified.insert(sid);
                    } else {
                        match mode {
                            ProcScanMode::Comb => {
                                disqualified.insert(sid);
                            }
                            ProcScanMode::Clocked(blk) => {
                                clocked_writers.entry(sid).or_default().insert(blk);
                            }
                            ProcScanMode::Initial => {}
                        }
                    }
                } else if t.kind(lhs) == SyntaxKind::MemberExpr {
                    if let Some(base) = t.children(lhs).find(|&k| t.kind(k) != SyntaxKind::Attribute) {
                        record_proc_lhs_write(
                            trees,
                            design,
                            inst_id,
                            scope_prefix,
                            locals,
                            t,
                            base,
                            indexed,
                            mode,
                            candidates,
                            disqualified,
                            clocked_writers,
                        );
                    }
                }
            } else if t.kind(lhs) == SyntaxKind::MemberExpr {
                if let Some(base) = t.children(lhs).find(|&k| t.kind(k) != SyntaxKind::Attribute) {
                    record_proc_lhs_write(
                        trees,
                        design,
                        inst_id,
                        scope_prefix,
                        locals,
                        t,
                        base,
                        indexed,
                        mode,
                        candidates,
                        disqualified,
                        clocked_writers,
                    );
                }
            }
        }
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn prescan_proc_node(
    trees: &[Tree],
    design: &ElaboratedDesign,
    inst_id: InstanceId,
    scope_prefix: &str,
    tree_idx: usize,
    node: NodeId,
    parent: Option<(NodeId, bool /* is_select_base */, bool /* in_sys_query */)>,
    mode: ProcScanMode,
    locals: &mut FxSet<String>,
    visited_subs: &mut FxSet<(usize, NodeId)>,
    candidates: &FxSet<SymbolId>,
    disqualified: &mut FxSet<SymbolId>,
    clocked_writers: &mut FxMap<SymbolId, FxSet<(InstanceId, usize)>>,
) {
    let t = &trees[tree_idx];
    let kind = t.kind(node);
    let in_sys_query = parent.is_some_and(|(_, _, q)| q);

    match kind {
        SyntaxKind::SeqBlock => {
            let saved = locals.clone();
            for c in t.children(node) {
                prescan_proc_node(
                    trees,
                    design,
                    inst_id,
                    scope_prefix,
                    tree_idx,
                    c,
                    Some((node, false, in_sys_query)),
                    mode,
                    locals,
                    visited_subs,
                    candidates,
                    disqualified,
                    clocked_writers,
                );
            }
            *locals = saved;
            return;
        }
        SyntaxKind::DataDecl | SyntaxKind::ParamDecl => {
            for decl in t.children(node).filter(|&c| t.kind(c) == SyntaxKind::Declarator) {
                for c in t.children(decl) {
                    if t.kind(c) != SyntaxKind::Name && t.kind(c) != SyntaxKind::Dim {
                        prescan_proc_node(
                            trees,
                            design,
                            inst_id,
                            scope_prefix,
                            tree_idx,
                            c,
                            Some((decl, false, in_sys_query)),
                            mode,
                            locals,
                            visited_subs,
                            candidates,
                            disqualified,
                            clocked_writers,
                        );
                    }
                }
                if let Some(nm_node) = t.child(decl, SyntaxKind::Name) {
                    let raw = t.bare_text(nm_node);
                    let nm = raw.trim().strip_prefix('\\').unwrap_or(raw.trim()).to_string();
                    if !nm.is_empty() {
                        locals.insert(nm);
                    }
                }
            }
            return;
        }
        SyntaxKind::LoopStmt => {
            let saved = locals.clone();
            if own_first_tok(t, node) == Some("foreach") {
                for ch in &t.node(node).children {
                    if let Child::Token(ti) = *ch {
                        let tk = &t.tokens[ti as usize];
                        if !tk.kind.is_trivia() {
                            let s = tk.text.trim();
                            if s.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '\\')
                                && s != "foreach"
                            {
                                // Add loop variable tokens after `[`
                            }
                        }
                    }
                }
                let mut in_bracket = false;
                for ch in &t.node(node).children {
                    if let Child::Token(ti) = *ch {
                        let tk = &t.tokens[ti as usize];
                        if tk.kind.is_trivia() {
                            continue;
                        }
                        let txt = &*tk.text;
                        if txt == "[" {
                            in_bracket = true;
                        } else if txt == "]" {
                            break;
                        } else if in_bracket && txt != "," {
                            locals.insert(txt.strip_prefix('\\').unwrap_or(txt).to_string());
                        }
                    }
                }
            }
            for c in t.children(node) {
                prescan_proc_node(
                    trees,
                    design,
                    inst_id,
                    scope_prefix,
                    tree_idx,
                    c,
                    Some((node, false, in_sys_query)),
                    mode,
                    locals,
                    visited_subs,
                    candidates,
                    disqualified,
                    clocked_writers,
                );
            }
            *locals = saved;
            return;
        }
        SyntaxKind::BinaryExpr => {
            let kids: Vec<NodeId> = t.children(node).filter(|&c| t.kind(c) != SyntaxKind::Attribute).collect();
            let op = t
                .node(node)
                .children
                .iter()
                .find_map(|c| match *c {
                    Child::Token(i) if !t.tokens[i as usize].kind.is_trivia() => Some(&*t.tokens[i as usize].text),
                    _ => None,
                })
                .unwrap_or("");
            let is_assign = matches!(
                op,
                "=" | "<=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=" | "<<<=" | ">>>="
            );
            if is_assign && kids.len() == 2 {
                record_proc_lhs_write(
                    trees,
                    design,
                    inst_id,
                    scope_prefix,
                    locals,
                    t,
                    kids[0],
                    false,
                    mode,
                    candidates,
                    disqualified,
                    clocked_writers,
                );
            }
        }
        SyntaxKind::PrefixExpr | SyntaxKind::PostfixExpr => {
            let op = own_first_tok(t, node).unwrap_or("");
            if matches!(op, "++" | "--") {
                if let Some(arg) = t.children(node).find(|&c| t.kind(c) != SyntaxKind::Attribute) {
                    record_proc_lhs_write(
                        trees,
                        design,
                        inst_id,
                        scope_prefix,
                        locals,
                        t,
                        arg,
                        false,
                        mode,
                        candidates,
                        disqualified,
                        clocked_writers,
                    );
                }
            }
        }
        SyntaxKind::NameExpr | SyntaxKind::MemberExpr => {
            let is_select_base = parent.is_some_and(|(_, sb, _)| sb);
            let parent_is_member = parent.is_some_and(|(p, _, _)| t.kind(p) == SyntaxKind::MemberExpr);
            if !is_select_base && !parent_is_member && !in_sys_query {
                if let Some(sid) = resolve_cst_ref_sym(trees, design, inst_id, scope_prefix, locals, t, node) {
                    if candidates.contains(&sid) {
                        disqualified.insert(sid);
                    }
                }
            }
            if kind == SyntaxKind::MemberExpr {
                return;
            }
        }
        SyntaxKind::CallExpr => {
            let callee_opt =
                t.children(node).find(|&c| !matches!(t.kind(c), SyntaxKind::ArgList | SyntaxKind::Attribute));
            let callee_txt = callee_opt.map(|c| t.bare_text(c)).unwrap_or_default();
            let callee_trimmed = callee_txt.trim();
            let is_sys_query = matches!(
                callee_trimmed,
                "$bits"
                    | "$size"
                    | "$left"
                    | "$right"
                    | "$low"
                    | "$high"
                    | "$increment"
                    | "$dimensions"
                    | "$unpacked_dimensions"
                    | "$typename"
            );
            if !callee_trimmed.starts_with('$') {
                if let Some(callee_n) = callee_opt {
                    if let Some((sub_kind, sub_tree, sub_decl, sub_inst_opt, sub_prefix)) =
                        resolve_call_target_in_scope(trees, design, inst_id, scope_prefix, t, callee_n)
                    {
                        let sub_inst = sub_inst_opt.unwrap_or(inst_id);
                        let is_task = sub_kind == ElabItemKind::TaskDecl;
                        let formals = sv_semantic::expr::extract_subroutine_formals(&trees[sub_tree], sub_decl);
                        if let Some(al) = t.child(node, SyntaxKind::ArgList) {
                            let args: Vec<NodeId> = t
                                .children(al)
                                .filter(|&c| t.kind(c) != SyntaxKind::Attribute)
                                .filter_map(|c| {
                                    if t.kind(c) == SyntaxKind::Arg {
                                        t.children(c).find(|&k| t.kind(k) != SyntaxKind::Attribute)
                                    } else {
                                        Some(c)
                                    }
                                })
                                .collect();
                            for (idx, &arg_n) in args.iter().enumerate() {
                                if let Some(f) = formals.get(idx) {
                                    if f.dir != PortDir::Input {
                                        record_proc_lhs_write(
                                            trees,
                                            design,
                                            inst_id,
                                            scope_prefix,
                                            locals,
                                            t,
                                            arg_n,
                                            false,
                                            mode,
                                            candidates,
                                            disqualified,
                                            clocked_writers,
                                        );
                                    }
                                }
                            }
                        }
                        if visited_subs.insert((sub_tree, sub_decl)) {
                            let mut sub_locals: FxSet<String> = formals.into_iter().map(|f| f.name).collect();
                            let sub_mode = if is_task { mode } else { ProcScanMode::Comb };
                            for c in trees[sub_tree].children(sub_decl) {
                                if !matches!(
                                    trees[sub_tree].kind(c),
                                    SyntaxKind::PortList | SyntaxKind::Name | SyntaxKind::DataType
                                ) {
                                    prescan_proc_node(
                                        trees,
                                        design,
                                        sub_inst,
                                        &sub_prefix,
                                        sub_tree,
                                        c,
                                        None,
                                        sub_mode,
                                        &mut sub_locals,
                                        visited_subs,
                                        candidates,
                                        disqualified,
                                        clocked_writers,
                                    );
                                }
                            }
                            visited_subs.remove(&(sub_tree, sub_decl));
                        }
                    }
                }
            }
            for c in t.children(node) {
                if Some(c) == callee_opt {
                    continue;
                }
                prescan_proc_node(
                    trees,
                    design,
                    inst_id,
                    scope_prefix,
                    tree_idx,
                    c,
                    Some((node, false, in_sys_query || is_sys_query)),
                    mode,
                    locals,
                    visited_subs,
                    candidates,
                    disqualified,
                    clocked_writers,
                );
            }
            return;
        }
        _ => {}
    }

    let first_non_attr = if kind == SyntaxKind::SelectExpr {
        t.children(node).find(|&c| t.kind(c) != SyntaxKind::Attribute)
    } else {
        None
    };
    for c in t.children(node) {
        let is_sb = Some(c) == first_non_attr;
        prescan_proc_node(
            trees,
            design,
            inst_id,
            scope_prefix,
            tree_idx,
            c,
            Some((node, is_sb, in_sys_query)),
            mode,
            locals,
            visited_subs,
            candidates,
            disqualified,
            clocked_writers,
        );
    }
}

trait SymbolNetInitExt {
    fn is_net_with_init(&self) -> bool;
}

impl SymbolNetInitExt for sv_semantic::Symbol {
    fn is_net_with_init(&self) -> bool {
        matches!(&self.kind, SymbolKind::Net { .. } | SymbolKind::Port { is_net: true, dir: PortDir::Output, .. })
    }
}

pub(crate) fn resolve_call_target_in_scope(
    trees: &[Tree],
    design: &ElaboratedDesign,
    inst_id: InstanceId,
    scope_prefix: &str,
    t: &Tree,
    callee_node: NodeId,
) -> Option<(ElabItemKind, usize, NodeId, Option<InstanceId>, String)> {
    let lookup_local_or_pkg = |target_inst: InstanceId, target_pfx: &str, pkg: Option<&str>, name: &str| {
        if pkg.is_none() {
            let inst = design.instance(target_inst);
            let mut pfx = target_pfx;
            loop {
                for item in &inst.items {
                    if matches!(item.kind, ElabItemKind::TaskDecl | ElabItemKind::FunctionDecl)
                        && item.scope_prefix == pfx
                        && sv_semantic::expr::subroutine_decl_name(&trees[item.tree], item.node).as_deref()
                            == Some(name)
                    {
                        return Some((item.kind, item.tree, item.node, Some(target_inst), pfx.to_string()));
                    }
                }
                if pfx.is_empty() {
                    break;
                }
                pfx = match pfx.rfind('.') {
                    Some(i) => &pfx[..i],
                    None => "",
                };
            }
        }
        design.resolve_subroutine(target_inst, target_pfx, pkg, name)
    };

    match t.kind(callee_node) {
        SyntaxKind::NameExpr => {
            let raw = t.bare_text(callee_node);
            let name = raw.trim().strip_prefix('\\').unwrap_or(raw.trim());
            lookup_local_or_pkg(inst_id, scope_prefix, None, name)
        }
        SyntaxKind::ScopedExpr => {
            let toks: Vec<&str> = t
                .token_range(callee_node)
                .into_iter()
                .filter(|&ti| !t.tokens[ti as usize].kind.is_trivia())
                .map(|ti| &*t.tokens[ti as usize].text)
                .collect();
            if let [p, "::", nm] = toks.as_slice() {
                let clean = nm.strip_prefix('\\').unwrap_or(nm);
                lookup_local_or_pkg(inst_id, scope_prefix, Some(p), clean)
            } else {
                None
            }
        }
        SyntaxKind::MemberExpr => {
            let raw = t.bare_text(callee_node);
            let compact: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
            let (inst_path, sub_raw) = compact.rsplit_once('.')?;
            let sub_name = sub_raw.strip_prefix('\\').unwrap_or(sub_raw);

            let (first_seg, rest_opt) = match inst_path.split_once('.') {
                Some((f, r)) => (f, Some(r)),
                None => (inst_path, None),
            };
            let clean_first = first_seg.strip_prefix('\\').unwrap_or(first_seg);

            // Check interface port handle first (`bus.write_val(...)`).
            if let Some(psid) = design.instance(inst_id).resolve_lexical(scope_prefix, clean_first) {
                if matches!(design.symbol(psid).ty, RtlType::Interface { .. }) {
                    if let Some((iface_id, _)) = resolve_interface_port(trees, design, inst_id, psid) {
                        let full_target = match rest_opt {
                            Some(r) => format!("{}.{r}", design.instance(iface_id).path),
                            None => design.instance(iface_id).path.clone(),
                        };
                        if let Some((tid, tpfx)) = sv_semantic::sva_ir::resolve_scope_path(design, &full_target) {
                            return lookup_local_or_pkg(tid, &tpfx, None, sub_name);
                        }
                    }
                }
            }

            // Otherwise resolve relative hierarchical scope (`u_child.my_task(...)` or `gen_blk.my_task(...)`).
            let cur_full = &design.instance(inst_id).path;
            let mut candidate_paths = Vec::new();
            if !scope_prefix.is_empty() {
                candidate_paths.push(format!("{cur_full}.{scope_prefix}.{inst_path}"));
            }
            candidate_paths.push(format!("{cur_full}.{inst_path}"));
            candidate_paths.push(inst_path.to_string());
            for cand in candidate_paths {
                if let Some((tid, tpfx)) = sv_semantic::sva_ir::resolve_scope_path(design, &cand) {
                    if let Some(res) = lookup_local_or_pkg(tid, &tpfx, None, sub_name) {
                        return Some(res);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

pub(crate) fn own_first_tok(t: &Tree, node: NodeId) -> Option<&str> {
    t.node(node).children.iter().find_map(|c| match *c {
        Child::Token(i) if !t.tokens[i as usize].kind.is_trivia() => Some(&*t.tokens[i as usize].text),
        _ => None,
    })
}

pub(crate) fn subtree_preorder(t: &Tree, root: NodeId) -> Vec<NodeId> {
    fn walk(t: &Tree, n: NodeId, out: &mut Vec<NodeId>) {
        out.push(n);
        for c in t.children(n) {
            walk(t, c, out);
        }
    }
    let _ = (addr_bits, compute_packed_slice_offset, compute_unpacked_index_bounds);
    let mut out = Vec::new();
    walk(t, root, &mut out);
    out
}
