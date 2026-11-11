//! Internal mutable graph builder (`NetBuilder`) with placeholder stitching,
//! 4-state constant folding, algebraic mux/slice simplification, and Tarjan SCC
//! combinational loop detection.

use sv_semantic::{BoundProperty, CVal, ExprArena, Loc, SampledKind};

use crate::ir::{
    Check, MemId, Memory, NetOp, Netlist, NetlistDiag, PortInput, PortOutput, SampledNode, State, StateId, TraceSignal,
    UndrivenSlice, ValueId, ValueKind, ValueNode,
};

#[derive(Clone, Debug)]
pub(crate) enum BuildKind {
    Placeholder,
    Alias(ValueId),
    Input(usize),
    Const(CVal),
    StateOut(StateId),
    Op { op: NetOp, args: Vec<ValueId> },
    MemRead { mem: MemId, addr: ValueId },
    Sampled(usize),
    Undriven(usize),
}

#[derive(Clone, Debug)]
pub(crate) struct BuildNode {
    pub width: u32,
    pub signed: bool,
    pub four_state: bool,
    pub kind: BuildKind,
    pub name: Option<String>,
    pub loc: Loc,
}

pub(crate) struct NetBuilder {
    pub top: String,
    pub files: Vec<String>,
    pub nodes: Vec<BuildNode>,
    pub inputs: Vec<PortInput>,
    pub outputs: Vec<PortOutput>,
    pub states: Vec<State>,
    pub memories: Vec<Memory>,
    pub sampled: Vec<SampledNode>,
    pub checks: Vec<Check>,
    pub undriven: Vec<UndrivenSlice>,
    pub traces: Vec<TraceSignal>,
    pub diags: Vec<NetlistDiag>,
}

impl NetBuilder {
    pub fn new(top: String, files: Vec<String>) -> Self {
        Self {
            top,
            files,
            nodes: Vec::new(),
            inputs: Vec::new(),
            outputs: Vec::new(),
            states: Vec::new(),
            memories: Vec::new(),
            sampled: Vec::new(),
            checks: Vec::new(),
            undriven: Vec::new(),
            traces: Vec::new(),
            diags: Vec::new(),
        }
    }

    pub fn from_netlist(nl: &Netlist) -> Self {
        let nodes = nl
            .values
            .iter()
            .map(|vn| {
                let kind = match &vn.kind {
                    ValueKind::Input(pi) => BuildKind::Input(*pi),
                    ValueKind::Const(cv) => BuildKind::Const(cv.clone()),
                    ValueKind::StateOut(sid) => BuildKind::StateOut(*sid),
                    ValueKind::Op { op, args } => BuildKind::Op { op: op.clone(), args: args.clone() },
                    ValueKind::MemRead { mem, addr } => BuildKind::MemRead { mem: *mem, addr: *addr },
                    ValueKind::Sampled(si) => BuildKind::Sampled(*si),
                    ValueKind::Undriven(ui) => BuildKind::Undriven(*ui),
                };
                BuildNode {
                    width: vn.width,
                    signed: vn.signed,
                    four_state: vn.four_state,
                    kind,
                    name: vn.name.clone(),
                    loc: vn.loc,
                }
            })
            .collect();
        Self {
            top: nl.top.clone(),
            files: nl.files.clone(),
            nodes,
            inputs: nl.inputs.clone(),
            outputs: nl.outputs.clone(),
            states: nl.states.clone(),
            memories: nl.memories.clone(),
            sampled: nl.sampled.clone(),
            checks: nl.checks.clone(),
            undriven: nl.undriven.clone(),
            traces: nl.traces.clone(),
            diags: Vec::new(),
        }
    }

    pub fn file_of(&self, loc: Loc) -> String {
        let f = match loc.expansion {
            Some((ef, _)) => ef,
            None => loc.file,
        };
        self.files.get(f as usize).cloned().unwrap_or_default()
    }

    pub fn err(&mut self, loc: Loc, msg: impl Into<String>) {
        let file = self.file_of(loc);
        self.diags.push(NetlistDiag { loc, file, message: msg.into(), unsupported: false });
    }

    pub fn unsup(&mut self, loc: Loc, msg: impl Into<String>) {
        let file = self.file_of(loc);
        self.diags.push(NetlistDiag { loc, file, message: msg.into(), unsupported: true });
    }

    pub fn alloc_raw(
        &mut self,
        width: u32,
        signed: bool,
        four_state: bool,
        kind: BuildKind,
        name: Option<String>,
        loc: Loc,
    ) -> ValueId {
        let id = ValueId(self.nodes.len() as u32);
        self.nodes.push(BuildNode { width: width.max(1), signed, four_state, kind, name, loc });
        id
    }

    pub fn alloc_placeholder(
        &mut self,
        width: u32,
        signed: bool,
        four_state: bool,
        name: Option<String>,
        loc: Loc,
    ) -> ValueId {
        self.alloc_raw(width, signed, four_state, BuildKind::Placeholder, name, loc)
    }

    pub fn bind_placeholder(&mut self, placeholder: ValueId, target: ValueId) {
        debug_assert!(matches!(self.nodes[placeholder.0 as usize].kind, BuildKind::Placeholder));
        if placeholder == target {
            return;
        }
        self.nodes[placeholder.0 as usize].kind = BuildKind::Alias(target);
    }

    pub fn alloc_const(&mut self, cval: CVal, four_state: bool, loc: Loc) -> ValueId {
        let cv = if four_state { cval } else { cval.two_state() };
        let w = cv.w.max(1);
        let s = cv.s;
        self.alloc_raw(w, s, four_state, BuildKind::Const(cv), None, loc)
    }

    pub fn const_cval(&self, mut id: ValueId) -> Option<&CVal> {
        for _ in 0..32 {
            match &self.nodes[id.0 as usize].kind {
                BuildKind::Const(cv) => return Some(cv),
                BuildKind::Alias(next) => id = *next,
                _ => return None,
            }
        }
        None
    }

    pub fn node_shape(&self, id: ValueId) -> (u32, bool, bool) {
        let n = &self.nodes[id.0 as usize];
        (n.width, n.signed, n.four_state)
    }

    pub fn alloc_op(
        &mut self,
        op: NetOp,
        args: Vec<ValueId>,
        width: u32,
        signed: bool,
        four_state: bool,
        loc: Loc,
    ) -> ValueId {
        // Constant folding and trivial identities while preserving 4-state X/Z and signedness.
        match (&op, args.as_slice()) {
            (NetOp::Slice { hi, lo }, &[a]) => {
                let (aw, a_signed, a_four) = self.node_shape(a);
                if *lo == 0 && *hi + 1 == aw && a_signed == signed && a_four == four_state {
                    return a;
                }
                if let Some(cv) = self.const_cval(a).cloned() {
                    let sliced = cv.slice(*lo, (*hi - *lo) + 1, signed);
                    return self.alloc_const(sliced, four_state, loc);
                }
                let inner_kind = self.nodes[a.0 as usize].kind.clone();
                if let BuildKind::Op { op: NetOp::Slice { lo: inner_lo, .. }, args: inner_args } = inner_kind {
                    if let &[inner_src] = inner_args.as_slice() {
                        return self.alloc_op(
                            NetOp::Slice { hi: inner_lo + *hi, lo: inner_lo + *lo },
                            vec![inner_src],
                            width,
                            signed,
                            four_state,
                            loc,
                        );
                    }
                } else if let BuildKind::Op { op: NetOp::Concat, args: concat_parts } = inner_kind {
                    // `Concat` is MSB-first; walk LSB-to-MSB (`concat_parts.iter().rev()`) and extract overlapping sub-slices.
                    let mut out_lsb_first: Vec<ValueId> = Vec::new();
                    let mut cur_lo = 0u32;
                    for &p in concat_parts.iter().rev() {
                        let (pw, _, pf4) = self.node_shape(p);
                        let cur_hi = cur_lo + pw - 1;
                        if cur_hi >= *lo && cur_lo <= *hi {
                            let sub_lo = (*lo).saturating_sub(cur_lo);
                            let sub_hi = (*hi).min(cur_hi) - cur_lo;
                            let sub_w = (sub_hi - sub_lo) + 1;
                            out_lsb_first.push(self.alloc_op(
                                NetOp::Slice { hi: sub_hi, lo: sub_lo },
                                vec![p],
                                sub_w,
                                false,
                                pf4,
                                loc,
                            ));
                        }
                        cur_lo += pw;
                    }
                    if out_lsb_first.len() == 1 {
                        let single = out_lsb_first[0];
                        let (_, ss, sf) = self.node_shape(single);
                        if ss == signed && sf == four_state {
                            return single;
                        }
                        return self.alloc_op(
                            NetOp::Slice { hi: width - 1, lo: 0 },
                            vec![single],
                            width,
                            signed,
                            four_state,
                            loc,
                        );
                    } else if !out_lsb_first.is_empty() {
                        out_lsb_first.reverse();
                        return self.alloc_op(NetOp::Concat, out_lsb_first, width, signed, four_state, loc);
                    }
                }
            }
            (NetOp::ZeroExt { width: tw }, &[a]) => {
                let (aw, a_signed, a_four) = self.node_shape(a);
                if *tw == aw && a_signed == signed && a_four == four_state {
                    return a;
                }
                if let Some(cv) = self.const_cval(a).cloned() {
                    let ext = cv.cast(aw, false).cast(*tw, false).cast(*tw, signed);
                    return self.alloc_const(ext, four_state, loc);
                }
            }
            (NetOp::SignExt { width: tw }, &[a]) => {
                let (aw, a_signed, a_four) = self.node_shape(a);
                if *tw == aw && a_signed == signed && a_four == four_state {
                    return a;
                }
                if let Some(cv) = self.const_cval(a).cloned() {
                    let ext = cv.cast(aw, true).cast(*tw, true).cast(*tw, signed);
                    return self.alloc_const(ext, four_state, loc);
                }
            }
            (NetOp::Mux, &[c, t, e]) => {
                let (_, ts, tf) = self.node_shape(t);
                let (_, es, ef) = self.node_shape(e);
                if t == e && ts == signed && tf == four_state {
                    return t;
                }
                if let Some(cc) = self.const_cval(c) {
                    if cc.known() {
                        let chosen = if cc.is_true() { t } else { e };
                        let (cw, cs, cf) = if cc.is_true() { (width, ts, tf) } else { (width, es, ef) };
                        if cs == signed && cf == four_state {
                            return chosen;
                        }
                        let _ = cw;
                        return self.alloc_op(
                            NetOp::Slice { hi: width - 1, lo: 0 },
                            vec![chosen],
                            width,
                            signed,
                            four_state,
                            loc,
                        );
                    }
                }
            }
            (NetOp::And, &[a, b]) => {
                if let Some(ca) = self.const_cval(a) {
                    if ca.known() && ca.i() == 0 && width == 1 && !self.node_shape(b).2 {
                        return self.alloc_const(CVal::new(1, signed, 0), false, loc);
                    }
                    if ca.known()
                        && width == 1
                        && ca.i() == 1
                        && self.node_shape(b).1 == signed
                        && self.node_shape(b).2 == four_state
                    {
                        return b;
                    }
                }
                if let Some(cb) = self.const_cval(b) {
                    if cb.known()
                        && width == 1
                        && cb.i() == 1
                        && self.node_shape(a).1 == signed
                        && self.node_shape(a).2 == four_state
                    {
                        return a;
                    }
                }
            }
            (NetOp::Or, &[a, b]) => {
                if let Some(ca) = self.const_cval(a) {
                    if ca.known()
                        && width == 1
                        && ca.i() == 0
                        && self.node_shape(b).1 == signed
                        && self.node_shape(b).2 == four_state
                    {
                        return b;
                    }
                }
                if let Some(cb) = self.const_cval(b) {
                    if cb.known()
                        && width == 1
                        && cb.i() == 0
                        && self.node_shape(a).1 == signed
                        && self.node_shape(a).2 == four_state
                    {
                        return a;
                    }
                }
            }
            (NetOp::Concat, parts) if parts.len() == 1 => {
                let p = parts[0];
                let (pw, ps, pf) = self.node_shape(p);
                if pw == width && ps == signed && pf == four_state {
                    return p;
                }
            }
            (NetOp::Concat, parts) if !parts.is_empty() && parts.iter().all(|&p| self.const_cval(p).is_some()) => {
                let mut acc: Option<CVal> = None;
                for &p in parts {
                    let c = self.const_cval(p).unwrap().clone();
                    acc = Some(match acc {
                        None => c,
                        Some(prev) => CVal::concat(&prev, &c),
                    });
                }
                if let Some(res) = acc {
                    return self.alloc_const(res.cast(width, signed), four_state, loc);
                }
            }
            _ => {}
        }

        // General 4-state constant folding for unary and binary `NetOp`s.
        if let &[a] = args.as_slice() {
            if let Some(ca) = self.const_cval(a).cloned() {
                if let Some(folded) = fold_unary_op(&op, ca, width, signed) {
                    return self.alloc_const(folded, four_state, loc);
                }
            }
        } else if let &[a, b] = args.as_slice() {
            if let (Some(ca), Some(cb)) = (self.const_cval(a).cloned(), self.const_cval(b).cloned()) {
                if let Some(folded) = fold_binary_op(&op, ca, cb, width, signed) {
                    return self.alloc_const(folded, four_state, loc);
                }
            }
        }
        self.alloc_raw(width, signed, four_state, BuildKind::Op { op, args }, None, loc)
    }

    pub fn alloc_bool(&mut self, val: ValueId, loc: Loc) -> ValueId {
        let (w, _, four) = self.node_shape(val);
        if w == 1 {
            return val;
        }
        if let Some(cv) = self.const_cval(val).cloned() {
            if let Some(r) = CVal::eval_unary("|", cv) {
                return self.alloc_const(r, four, loc);
            }
        }
        self.alloc_op(NetOp::RedOr, vec![val], 1, false, four, loc)
    }

    pub fn alloc_not1(&mut self, val: ValueId, loc: Loc) -> ValueId {
        let b = self.alloc_bool(val, loc);
        let (_, _, four) = self.node_shape(b);
        if let Some(cv) = self.const_cval(b).cloned() {
            if let Some(r) = CVal::eval_unary("!", cv) {
                return self.alloc_const(r, four, loc);
            }
        }
        self.alloc_op(NetOp::LogNot, vec![b], 1, false, four, loc)
    }

    pub fn alloc_and1(&mut self, a: ValueId, b: ValueId, loc: Loc) -> ValueId {
        let ba = self.alloc_bool(a, loc);
        let bb = self.alloc_bool(b, loc);
        if ba == bb {
            return ba;
        }
        let four = self.node_shape(ba).2 || self.node_shape(bb).2;
        if let (Some(ca), Some(cb)) = (self.const_cval(ba).cloned(), self.const_cval(bb).cloned()) {
            if let Some(r) = CVal::eval_binary("&&", ca, cb) {
                return self.alloc_const(r, four, loc);
            }
        }
        if let Some(ca) = self.const_cval(ba) {
            if ca.known() {
                return if ca.is_true() { bb } else { self.alloc_const(CVal::new(1, false, 0), false, loc) };
            }
        }
        if let Some(cb) = self.const_cval(bb) {
            if cb.known() {
                return if cb.is_true() { ba } else { self.alloc_const(CVal::new(1, false, 0), false, loc) };
            }
        }
        self.alloc_op(NetOp::LogAnd, vec![ba, bb], 1, false, four, loc)
    }

    pub fn alloc_or1(&mut self, a: ValueId, b: ValueId, loc: Loc) -> ValueId {
        let ba = self.alloc_bool(a, loc);
        let bb = self.alloc_bool(b, loc);
        if ba == bb {
            return ba;
        }
        let four = self.node_shape(ba).2 || self.node_shape(bb).2;
        if let (Some(ca), Some(cb)) = (self.const_cval(ba).cloned(), self.const_cval(bb).cloned()) {
            if let Some(r) = CVal::eval_binary("||", ca, cb) {
                return self.alloc_const(r, four, loc);
            }
        }
        if let Some(ca) = self.const_cval(ba) {
            if ca.known() {
                return if ca.is_true() { self.alloc_const(CVal::new(1, false, 1), false, loc) } else { bb };
            }
        }
        if let Some(cb) = self.const_cval(bb) {
            if cb.known() {
                return if cb.is_true() { self.alloc_const(CVal::new(1, false, 1), false, loc) } else { ba };
            }
        }
        self.alloc_op(NetOp::LogOr, vec![ba, bb], 1, false, four, loc)
    }

    pub fn resize_val(&mut self, val: ValueId, target_w: u32, signed_ext: bool, out_signed: bool, loc: Loc) -> ValueId {
        let (cur_w, cur_s, four) = self.node_shape(val);
        let target_w = target_w.max(1);
        if cur_w == target_w {
            if cur_s == out_signed {
                return val;
            }
            return self.alloc_op(NetOp::Slice { hi: target_w - 1, lo: 0 }, vec![val], target_w, out_signed, four, loc);
        }
        if target_w < cur_w {
            self.alloc_op(NetOp::Slice { hi: target_w - 1, lo: 0 }, vec![val], target_w, out_signed, four, loc)
        } else if signed_ext {
            self.alloc_op(NetOp::SignExt { width: target_w }, vec![val], target_w, out_signed, four, loc)
        } else {
            self.alloc_op(NetOp::ZeroExt { width: target_w }, vec![val], target_w, out_signed, four, loc)
        }
    }

    /// Detects combinational loops via iterative Tarjan SCC, resolves all `Alias` nodes,
    /// and returns a topologically sorted [`Netlist`].
    pub fn finalize(
        mut self,
        sva_arena: ExprArena,
        properties: Vec<BoundProperty>,
        mut prop_expr_values: Vec<Option<ValueId>>,
        mut symbol_values: Vec<(sv_semantic::SymbolId, ValueId)>,
    ) -> Result<Netlist, Vec<NetlistDiag>> {
        if !self.diags.is_empty() {
            return Err(self.diags);
        }

        // Convert any remaining unbound `Placeholder` into an `Undriven` node so no panics can occur.
        for i in 0..self.nodes.len() {
            if matches!(self.nodes[i].kind, BuildKind::Placeholder) {
                let uid = self.undriven.len();
                let sig_name = self.nodes[i].name.clone().unwrap_or_else(|| format!("_unbound_{i}"));
                let w = self.nodes[i].width;
                let loc = self.nodes[i].loc;
                self.nodes[i].kind = BuildKind::Undriven(uid);
                self.undriven.push(UndrivenSlice { signal: sig_name, hi: w - 1, lo: 0, value: ValueId(i as u32), loc });
            }
        }

        // Combinational cycle check (iterative Tarjan SCC).
        self.check_combinational_loops()?;
        if !self.diags.is_empty() {
            return Err(self.diags);
        }

        // Chase all `Alias` chains to their concrete non-alias `ValueId`.
        let n = self.nodes.len();
        let mut resolved = vec![ValueId(0); n];
        for (i, res_slot) in resolved.iter_mut().enumerate() {
            let mut cur = ValueId(i as u32);
            while let BuildKind::Alias(next) = self.nodes[cur.0 as usize].kind {
                cur = next;
            }
            if self.nodes[cur.0 as usize].name.is_none() {
                if let Some(nm) = self.nodes[i].name.clone() {
                    self.nodes[cur.0 as usize].name = Some(nm);
                }
            }
            *res_slot = cur;
        }

        let mut values = Vec::with_capacity(n);
        for (i, bn) in self.nodes.iter().enumerate() {
            let kind = match &bn.kind {
                BuildKind::Placeholder => unreachable!(),
                BuildKind::Alias(_) => ValueKind::Const(CVal::new(bn.width, bn.signed, 0)),
                BuildKind::Input(pi) => ValueKind::Input(*pi),
                BuildKind::Const(cv) => ValueKind::Const(cv.clone()),
                BuildKind::StateOut(sid) => ValueKind::StateOut(*sid),
                BuildKind::Op { op, args } => {
                    ValueKind::Op { op: op.clone(), args: args.iter().map(|a| resolved[a.0 as usize]).collect() }
                }
                BuildKind::MemRead { mem, addr } => ValueKind::MemRead { mem: *mem, addr: resolved[addr.0 as usize] },
                BuildKind::Sampled(si) => ValueKind::Sampled(*si),
                BuildKind::Undriven(ui) => ValueKind::Undriven(*ui),
            };
            values.push(ValueNode {
                id: ValueId(i as u32),
                width: bn.width,
                signed: bn.signed,
                four_state: bn.four_state,
                kind,
                name: bn.name.clone(),
                loc: bn.loc,
            });
        }

        let map_r = |v: &mut ValueId| *v = resolved[v.0 as usize];
        for p in &mut self.inputs {
            map_r(&mut p.value);
        }
        for o in &mut self.outputs {
            map_r(&mut o.value);
        }
        for s in &mut self.states {
            if let Some(clk) = &mut s.clock {
                map_r(&mut clk.signal);
            }
            if let Some(ar) = &mut s.async_reset {
                map_r(&mut ar.signal);
                map_r(&mut ar.value);
            }
            if let Some(le) = &mut s.latch_enable {
                map_r(le);
            }
            map_r(&mut s.next);
            map_r(&mut s.out);
        }
        for m in &mut self.memories {
            for wp in &mut m.writes {
                if let Some(clk) = &mut wp.clock {
                    map_r(&mut clk.signal);
                }
                map_r(&mut wp.addr);
                map_r(&mut wp.data);
                map_r(&mut wp.enable);
            }
        }
        for sn in &mut self.sampled {
            map_r(&mut sn.expr);
            if let Some(clk) = &mut sn.clock {
                map_r(&mut clk.signal);
            }
            if let Some(g) = &mut sn.gate {
                map_r(g);
            }
            map_r(&mut sn.out);
        }
        for c in &mut self.checks {
            map_r(&mut c.enable);
            map_r(&mut c.expr);
            if let Some(clk) = &mut c.clock {
                map_r(&mut clk.signal);
            }
        }
        for u in &mut self.undriven {
            map_r(&mut u.value);
        }
        for tr in &mut self.traces {
            map_r(&mut tr.value);
        }
        for pv in prop_expr_values.iter_mut().flatten() {
            map_r(pv);
        }
        for (_, sv) in &mut symbol_values {
            map_r(sv);
        }

        let mut netlist = Netlist {
            top: self.top,
            files: self.files,
            values,
            inputs: self.inputs,
            outputs: self.outputs,
            states: self.states,
            memories: self.memories,
            sampled: self.sampled,
            checks: self.checks,
            undriven: self.undriven,
            traces: self.traces,
            sva_arena,
            properties,
            prop_expr_values,
            symbol_values,
        };
        netlist.toposort();
        Ok(netlist)
    }

    fn comb_deps(&self, idx: usize, out: &mut Vec<usize>) {
        out.clear();
        match &self.nodes[idx].kind {
            BuildKind::Placeholder
            | BuildKind::Input(_)
            | BuildKind::Const(_)
            | BuildKind::StateOut(_)
            | BuildKind::Undriven(_) => {}
            BuildKind::Alias(t) => out.push(t.0 as usize),
            BuildKind::Op { args, .. } => {
                for a in args {
                    out.push(a.0 as usize);
                }
            }
            BuildKind::MemRead { mem, addr } => {
                out.push(addr.0 as usize);
                for wp in &self.memories[mem.0 as usize].writes {
                    if wp.clock.is_none() {
                        out.push(wp.addr.0 as usize);
                        out.push(wp.data.0 as usize);
                        out.push(wp.enable.0 as usize);
                    }
                }
            }
            BuildKind::Sampled(si) => {
                let sn = &self.sampled[*si];
                // `$past(e, n)` with `n >= 1` reads previous clock cycles, breaking combinational loops.
                if sn.kind != SampledKind::Past || sn.depth == 0 {
                    out.push(sn.expr.0 as usize);
                }
            }
        }
    }

    fn check_combinational_loops(&mut self) -> Result<(), Vec<NetlistDiag>> {
        struct Frame {
            v: usize,
            deps: Vec<usize>,
            dep_idx: usize,
            self_loop: bool,
        }

        let n = self.nodes.len();
        let mut cur_index = 0u32;
        let mut indices = vec![u32::MAX; n];
        let mut lowlink = vec![u32::MAX; n];
        let mut on_stack = vec![false; n];
        let mut stack: Vec<usize> = Vec::new();
        let mut work: Vec<Frame> = Vec::new();
        let mut cycles: Vec<Vec<usize>> = Vec::new();

        for start in 0..n {
            if indices[start] != u32::MAX {
                continue;
            }
            indices[start] = cur_index;
            lowlink[start] = cur_index;
            cur_index += 1;
            stack.push(start);
            on_stack[start] = true;
            let mut start_deps = Vec::new();
            self.comb_deps(start, &mut start_deps);
            work.push(Frame { v: start, deps: start_deps, dep_idx: 0, self_loop: false });

            while let Some(top) = work.last_mut() {
                if top.dep_idx < top.deps.len() {
                    let v = top.v;
                    let w = top.deps[top.dep_idx];
                    top.dep_idx += 1;
                    if w == v {
                        top.self_loop = true;
                    }
                    if indices[w] == u32::MAX {
                        indices[w] = cur_index;
                        lowlink[w] = cur_index;
                        cur_index += 1;
                        stack.push(w);
                        on_stack[w] = true;
                        let mut w_deps = Vec::new();
                        self.comb_deps(w, &mut w_deps);
                        work.push(Frame { v: w, deps: w_deps, dep_idx: 0, self_loop: false });
                    } else if on_stack[w] {
                        lowlink[v] = lowlink[v].min(indices[w]);
                    }
                } else {
                    let finished = work.pop().unwrap();
                    let v = finished.v;
                    if let Some(parent) = work.last() {
                        let pv = parent.v;
                        lowlink[pv] = lowlink[pv].min(lowlink[v]);
                    }
                    if lowlink[v] == indices[v] {
                        let mut scc = Vec::new();
                        while let Some(w) = stack.pop() {
                            on_stack[w] = false;
                            scc.push(w);
                            if w == v {
                                break;
                            }
                        }
                        if scc.len() > 1 || finished.self_loop {
                            cycles.push(scc);
                        }
                    }
                }
            }
        }

        for scc in cycles {
            let mut named: Vec<(String, Loc)> = Vec::new();
            for &idx in scc.iter().rev() {
                if let Some(nm) = &self.nodes[idx].name {
                    if !named.iter().any(|(existing, _)| existing == nm) {
                        named.push((nm.clone(), self.nodes[idx].loc));
                    }
                }
            }
            let (loc, cycle_str) = if let Some((first_nm, first_loc)) = named.first().cloned() {
                let mut names: Vec<String> = named.into_iter().map(|(nm, _)| nm).collect();
                names.push(first_nm);
                (first_loc, names.join(" -> "))
            } else {
                let first_idx = scc[0];
                (self.nodes[first_idx].loc, "<anonymous combinational expression>".to_string())
            };
            self.err(loc, format!("combinational loop detected: {cycle_str}"));
        }
        Ok(())
    }
}

fn fold_unary_op(op: &NetOp, a: CVal, width: u32, signed: bool) -> Option<CVal> {
    let r = match op {
        NetOp::Not => CVal::eval_unary("~", a)?,
        NetOp::Neg => CVal::eval_unary("-", a)?,
        NetOp::RedAnd => CVal::eval_unary("&", a)?,
        NetOp::RedOr => CVal::eval_unary("|", a)?,
        NetOp::RedXor => CVal::eval_unary("^", a)?,
        NetOp::LogNot => CVal::eval_unary("!", a)?,
        NetOp::WildEq { rhs } => CVal::eval_binary("==?", a, rhs.clone())?,
        NetOp::WildNeq { rhs } => CVal::eval_binary("!=?", a, rhs.clone())?,
        NetOp::CastFourState { four_state } => {
            if *four_state {
                a
            } else {
                a.two_state()
            }
        }
        _ => return None,
    };
    Some(r.cast(width, signed))
}

fn fold_binary_op(op: &NetOp, a: CVal, b: CVal, width: u32, signed: bool) -> Option<CVal> {
    let aw = a.w;
    let bw = b.w;
    let r = match op {
        NetOp::Add => CVal::eval_binary("+", a, b)?,
        NetOp::Sub => CVal::eval_binary("-", a, b)?,
        NetOp::Mul => CVal::eval_binary("*", a, b)?,
        NetOp::UDiv => CVal::eval_binary("/", a.cast(aw, false), b.cast(bw, false))?,
        NetOp::SDiv => CVal::eval_binary("/", a.cast(aw, true), b.cast(bw, true))?,
        NetOp::UMod => CVal::eval_binary("%", a.cast(aw, false), b.cast(bw, false))?,
        NetOp::SMod => CVal::eval_binary("%", a.cast(aw, true), b.cast(bw, true))?,
        NetOp::Pow => CVal::eval_binary("**", a, b)?,
        NetOp::And => CVal::eval_binary("&", a, b)?,
        NetOp::Or => CVal::eval_binary("|", a, b)?,
        NetOp::Xor => CVal::eval_binary("^", a, b)?,
        NetOp::Xnor => CVal::eval_binary("~^", a, b)?,
        NetOp::LogAnd => CVal::eval_binary("&&", a, b)?,
        NetOp::LogOr => CVal::eval_binary("||", a, b)?,
        NetOp::Shl => CVal::eval_binary("<<", a, b.cast(bw, false))?,
        NetOp::LShr => CVal::eval_binary(">>", a.cast(aw, false), b.cast(bw, false))?,
        NetOp::AShr => CVal::eval_binary(">>>", a.cast(aw, true), b.cast(bw, false))?,
        NetOp::Eq => CVal::eval_binary("==", a, b)?,
        NetOp::Neq => CVal::eval_binary("!=", a, b)?,
        NetOp::CaseEq => CVal::eval_binary("===", a, b)?,
        NetOp::CaseNeq => CVal::eval_binary("!==", a, b)?,
        NetOp::ULt => CVal::eval_binary("<", a.cast(aw, false), b.cast(bw, false))?,
        NetOp::SLt => CVal::eval_binary("<", a.cast(aw, true), b.cast(bw, true))?,
        NetOp::ULe => CVal::eval_binary("<=", a.cast(aw, false), b.cast(bw, false))?,
        NetOp::SLe => CVal::eval_binary("<=", a.cast(aw, true), b.cast(bw, true))?,
        _ => return None,
    };
    Some(r.cast(width, signed))
}
