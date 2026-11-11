//! 4-state, word-level cycle-accurate interpreter for `sv_netlist::Netlist`.
//!
//! Used by integration tests to verify that lowered netlists not only have the
//! expected structural IR, but compute exact IEEE 1800 4-state results across
//! combinational evaluation, synchronous clock edges, asynchronous resets,
//! transparent latches, memories, and immediate assertions.

#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};

use sv_netlist::{CheckKind, ClockSpec, Edge, LowerConfig, LowerOutcome, NetOp, Netlist, ValueId, ValueKind};
use sv_semantic::{BitState, CVal, SampledKind};

pub fn lower_src(src: &str) -> LowerOutcome {
    lower_src_cfg(src, &[], &LowerConfig::default())
}

pub fn lower_src_cfg(src: &str, params: &[(&str, &str)], cfg: &LowerConfig) -> LowerOutcome {
    let tree = sv_syntax::parse_source(src, "test.sv", sv_syntax::Options::default());
    let owned_params: Vec<(String, String)> =
        params.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();
    sv_netlist::lower_trees(&[tree], &[], &owned_params, cfg)
}

pub fn lower_testdata(filename: &str, params: &[(&str, &str)], cfg: &LowerConfig) -> LowerOutcome {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/netlist").join(filename);
    let tree = sv_syntax::parse_file(&path, sv_syntax::Options::default());
    let owned_params: Vec<(String, String)> =
        params.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect();
    sv_netlist::lower_trees(&[tree], &[], &owned_params, cfg)
}

pub struct NetlistSim<'a> {
    pub nl: &'a Netlist,
    pub inputs: HashMap<String, CVal>,
    pub states: Vec<CVal>,
    pub mems: Vec<Vec<CVal>>,
    pub vals: Vec<CVal>,
    pub prev_vals: Vec<CVal>,
    pub sampled_hist: Vec<VecDeque<CVal>>,
    pub failed_asserts: Vec<String>,
}

impl<'a> NetlistSim<'a> {
    pub fn new(nl: &'a Netlist) -> Self {
        // Verify strict topological invariant on `nl.values` up front!
        assert_topological_order(nl);

        let mut inputs = HashMap::new();
        for p in &nl.inputs {
            inputs.insert(p.name.clone(), CVal::new(p.width, p.signed, 0));
        }

        let states: Vec<CVal> = nl
            .states
            .iter()
            .map(|s| {
                s.init.clone().unwrap_or_else(|| {
                    if s.four_state {
                        CVal::all_x(s.width, s.signed)
                    } else {
                        CVal::new(s.width, s.signed, 0)
                    }
                })
            })
            .collect();

        let mems: Vec<Vec<CVal>> = nl
            .memories
            .iter()
            .map(|m| {
                let def = if m.four_state { CVal::all_x(m.width, m.signed) } else { CVal::new(m.width, m.signed, 0) };
                let mut words = vec![def; m.depth as usize];
                for (idx, cv) in &m.init {
                    if (*idx as usize) < words.len() {
                        words[*idx as usize] = cv.clone();
                    }
                }
                words
            })
            .collect();

        let vals: Vec<CVal> = nl.values.iter().map(|v| CVal::new(v.width, v.signed, 0)).collect();
        let sampled_hist = vec![VecDeque::new(); nl.sampled.len()];

        let mut sim =
            Self { nl, inputs, states, mems, prev_vals: vals.clone(), vals, sampled_hist, failed_asserts: Vec::new() };
        sim.eval_comb();
        sim
    }

    pub fn set_u(&mut self, port: &str, val: i128) {
        let p = self.nl.inputs.iter().find(|i| i.name == port).expect("unknown input port");
        self.inputs.insert(port.to_string(), CVal::new(p.width, p.signed, val));
    }

    pub fn set_cval(&mut self, port: &str, cv: CVal) {
        let p = self.nl.inputs.iter().find(|i| i.name == port).expect("unknown input port");
        self.inputs.insert(port.to_string(), cv.cast(p.width, p.signed));
    }

    pub fn out_cval(&mut self, port: &str) -> CVal {
        self.eval_comb();
        let o = self.nl.outputs.iter().find(|o| o.name == port).expect("unknown output port");
        self.vals[o.value.0 as usize].clone()
    }

    pub fn out_i(&mut self, port: &str) -> i128 {
        let cv = self.out_cval(port);
        assert!(cv.known(), "expected known output on '{port}', got X/Z ({cv:?})");
        cv.i()
    }

    /// Evaluates combinational logic, asynchronous resets, and transparent latches to a fixed point.
    pub fn eval_comb(&mut self) {
        for _ in 0..4 {
            for node in &self.nl.values {
                let idx = node.id.0 as usize;
                let mut cv = match &node.kind {
                    ValueKind::Input(pi) => self.inputs[&self.nl.inputs[*pi].name].clone(),
                    ValueKind::Const(c) => c.clone(),
                    ValueKind::StateOut(sid) => self.states[sid.0 as usize].clone(),
                    ValueKind::Undriven(_) => CVal::new(node.width, node.signed, 0),
                    ValueKind::MemRead { mem, addr } => {
                        let ac = &self.vals[addr.0 as usize];
                        let words = &self.mems[mem.0 as usize];
                        if !ac.known() || ac.i() < 0 || (ac.i() as usize) >= words.len() {
                            if node.four_state {
                                CVal::all_x(node.width, node.signed)
                            } else {
                                CVal::new(node.width, node.signed, 0)
                            }
                        } else {
                            words[ac.i() as usize].clone()
                        }
                    }
                    ValueKind::Sampled(si) => self.eval_sampled_val(*si, node.width, node.signed),
                    ValueKind::Op { op, args } => eval_net_op(op, args, &self.vals, node.width, node.signed),
                };
                if !node.four_state {
                    cv = cv.two_state();
                }
                self.vals[idx] = cv.cast(node.width, node.signed);
            }

            // Apply asynchronous resets and transparent latches immediately!
            let mut changed = false;
            for s in &self.nl.states {
                if let Some(ar) = s.async_reset {
                    let sig = &self.vals[ar.signal.0 as usize];
                    let active =
                        sig.known() && ((ar.active_high && sig.is_true()) || (!ar.active_high && !sig.is_true()));
                    if active {
                        let rv = self.vals[ar.value.0 as usize].cast(s.width, s.signed);
                        if self.states[s.id.0 as usize] != rv {
                            self.states[s.id.0 as usize] = rv;
                            changed = true;
                        }
                    }
                } else if let Some(le) = s.latch_enable {
                    let en = &self.vals[le.0 as usize];
                    if en.known() && en.is_true() {
                        let nv = self.vals[s.next.0 as usize].cast(s.width, s.signed);
                        if self.states[s.id.0 as usize] != nv {
                            self.states[s.id.0 as usize] = nv;
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// Toggles `clk_port` (`0 -> 1` or `1 -> 0`) or steps a full `0 -> 1` posedge tick on `clk_port`.
    pub fn tick_posedge(&mut self, clk_port: &str) {
        self.set_u(clk_port, 0);
        self.eval_comb();
        self.prev_vals = self.vals.clone();
        self.set_u(clk_port, 1);
        self.step_edges();
    }

    pub fn set_and_step(&mut self, port: &str, val: i128) {
        self.eval_comb();
        self.prev_vals = self.vals.clone();
        self.set_u(port, val);
        self.step_edges();
    }

    fn step_edges(&mut self) {
        // Re-evaluate combinational clocks (e.g. primary inputs first, or derived clocks after register update).
        for _pass in 0..2 {
            self.eval_comb();
            let edge_fired = |clk: ClockSpec, prev: &[CVal], cur: &[CVal]| -> bool {
                let p = &prev[clk.signal.0 as usize];
                let c = &cur[clk.signal.0 as usize];
                if !p.known() || !c.known() {
                    return false;
                }
                match clk.edge {
                    Edge::Posedge => !p.is_true() && c.is_true(),
                    Edge::Negedge => p.is_true() && !c.is_true(),
                }
            };

            // Evaluate checks on active clock edges (using pre-update values!).
            for chk in &self.nl.checks {
                let fire = match chk.clock {
                    Some(clk) => edge_fired(clk, &self.prev_vals, &self.vals),
                    None => true,
                };
                if fire && chk.kind == CheckKind::Assert {
                    let en = &self.prev_vals[chk.enable.0 as usize];
                    let ex = &self.prev_vals[chk.expr.0 as usize];
                    if en.known() && en.is_true() && (!ex.known() || !ex.is_true()) {
                        self.failed_asserts.push(chk.label.clone().unwrap_or_else(|| "<unnamed>".into()));
                    }
                }
            }

            // Sample `$past` history on active edges.
            for (si, sn) in self.nl.sampled.iter().enumerate() {
                if let Some(clk) = sn.clock {
                    if edge_fired(clk, &self.prev_vals, &self.vals) {
                        let gate_ok = sn.gate.is_none_or(|g| {
                            self.prev_vals[g.0 as usize].known() && self.prev_vals[g.0 as usize].is_true()
                        });
                        if gate_ok {
                            self.sampled_hist[si].push_front(self.prev_vals[sn.expr.0 as usize].clone());
                            while self.sampled_hist[si].len() > 16 {
                                self.sampled_hist[si].pop_back();
                            }
                        }
                    }
                }
            }

            // Stage register updates (`prev_vals` holds pre-clock combinational values!).
            let mut staged_states = self.states.clone();
            let mut any_state_updated = false;
            for s in &self.nl.states {
                let Some(clk) = s.clock else { continue };
                if !edge_fired(clk, &self.prev_vals, &self.vals) {
                    continue;
                }
                if let Some(ar) = s.async_reset {
                    let sig = &self.vals[ar.signal.0 as usize];
                    let active =
                        sig.known() && ((ar.active_high && sig.is_true()) || (!ar.active_high && !sig.is_true()));
                    if active {
                        continue;
                    }
                }
                let next_cv = self.prev_vals[s.next.0 as usize].cast(s.width, s.signed);
                if staged_states[s.id.0 as usize] != next_cv {
                    staged_states[s.id.0 as usize] = next_cv;
                    any_state_updated = true;
                }
            }

            // Stage synchronous memory writes.
            for (mi, m) in self.nl.memories.iter().enumerate() {
                for wp in &m.writes {
                    let Some(clk) = wp.clock else { continue };
                    if !edge_fired(clk, &self.prev_vals, &self.vals) {
                        continue;
                    }
                    let en = &self.prev_vals[wp.enable.0 as usize];
                    if en.known() && en.is_true() {
                        let addr = &self.prev_vals[wp.addr.0 as usize];
                        if addr.known() && addr.i() >= 0 && (addr.i() as usize) < self.mems[mi].len() {
                            self.mems[mi][addr.i() as usize] =
                                self.prev_vals[wp.data.0 as usize].cast(m.width, m.signed);
                        }
                    }
                }
            }

            // Update `prev_vals` BEFORE committing `staged_states` in case a derived clock (`div_clk`) toggles!
            let before_commit = self.vals.clone();
            self.states = staged_states;
            self.eval_comb();
            if !any_state_updated {
                break;
            }
            self.prev_vals = before_commit;
        }
    }

    fn eval_sampled_val(&self, si: usize, width: u32, signed: bool) -> CVal {
        let sn = &self.nl.sampled[si];
        let past_idx = sn.depth.saturating_sub(1) as usize;
        if sn.kind == SampledKind::Past {
            return self
                .sampled_hist
                .get(si)
                .and_then(|h| h.get(past_idx))
                .cloned()
                .unwrap_or_else(|| CVal::all_x(width, signed))
                .cast(width, signed);
        }
        let cur = &self.vals[sn.expr.0 as usize];
        let past = self
            .sampled_hist
            .get(si)
            .and_then(|h| h.get(past_idx))
            .cloned()
            .unwrap_or_else(|| CVal::all_x(cur.w, cur.s));
        match sn.kind {
            SampledKind::Sampled => cur.cast(width, signed),
            SampledKind::Past => unreachable!(),
            SampledKind::Rose => CVal::bool(past.bit_state(0) != BitState::One && cur.bit_state(0) == BitState::One),
            SampledKind::Fell => CVal::bool(past.bit_state(0) != BitState::Zero && cur.bit_state(0) == BitState::Zero),
            SampledKind::Stable => CVal::bool(past == *cur),
            SampledKind::Changed => CVal::bool(past != *cur),
        }
    }
}

pub fn assert_topological_order(nl: &Netlist) {
    for (idx, node) in nl.values.iter().enumerate() {
        assert_eq!(node.id, ValueId(idx as u32), "ValueNode::id must match slice index");
        match &node.kind {
            ValueKind::Op { args, .. } => {
                for &a in args {
                    assert!((a.0 as usize) < idx, "topological order violated at v{idx}: references operand v{}", a.0);
                }
            }
            ValueKind::MemRead { addr, .. } => {
                assert!(
                    (addr.0 as usize) < idx,
                    "topological order violated at MemRead v{idx}: references addr v{}",
                    addr.0
                );
            }
            ValueKind::Sampled(si) => {
                let sn = &nl.sampled[*si];
                if sn.kind != SampledKind::Past {
                    assert!((sn.expr.0 as usize) < idx, "topological order violated at Sampled v{idx}");
                }
            }
            _ => {}
        }
    }
}

fn eval_net_op(op: &NetOp, args: &[ValueId], vals: &[CVal], w: u32, s: bool) -> CVal {
    let a = |i: usize| vals[args[i].0 as usize].clone();
    match op {
        NetOp::Add => CVal::eval_binary("+", a(0), a(1)).unwrap().cast(w, s),
        NetOp::Sub => CVal::eval_binary("-", a(0), a(1)).unwrap().cast(w, s),
        NetOp::Mul => CVal::eval_binary("*", a(0), a(1)).unwrap().cast(w, s),
        NetOp::UDiv => CVal::eval_binary("/", a(0).cast(w, false), a(1).cast(w, false)).unwrap().cast(w, s),
        NetOp::SDiv => CVal::eval_binary("/", a(0).cast(w, true), a(1).cast(w, true)).unwrap().cast(w, s),
        NetOp::UMod => CVal::eval_binary("%", a(0).cast(w, false), a(1).cast(w, false)).unwrap().cast(w, s),
        NetOp::SMod => CVal::eval_binary("%", a(0).cast(w, true), a(1).cast(w, true)).unwrap().cast(w, s),
        NetOp::Pow => CVal::eval_binary("**", a(0), a(1)).unwrap().cast(w, s),
        NetOp::And => CVal::eval_binary("&", a(0), a(1)).unwrap().cast(w, s),
        NetOp::Or => CVal::eval_binary("|", a(0), a(1)).unwrap().cast(w, s),
        NetOp::Xor => CVal::eval_binary("^", a(0), a(1)).unwrap().cast(w, s),
        NetOp::Xnor => CVal::eval_binary("~^", a(0), a(1)).unwrap().cast(w, s),
        NetOp::Not => CVal::eval_unary("~", a(0)).unwrap().cast(w, s),
        NetOp::Neg => CVal::eval_unary("-", a(0)).unwrap().cast(w, s),
        NetOp::RedAnd => CVal::eval_unary("&", a(0)).unwrap(),
        NetOp::RedOr => CVal::eval_unary("|", a(0)).unwrap(),
        NetOp::RedXor => CVal::eval_unary("^", a(0)).unwrap(),
        NetOp::LogNot => CVal::eval_unary("!", a(0)).unwrap(),
        NetOp::LogAnd => CVal::eval_binary("&&", a(0), a(1)).unwrap(),
        NetOp::LogOr => CVal::eval_binary("||", a(0), a(1)).unwrap(),
        NetOp::Shl => CVal::eval_binary("<<", a(0), a(1)).unwrap().cast(w, s),
        NetOp::LShr => CVal::eval_binary(">>", a(0).cast(w, false), a(1)).unwrap().cast(w, s),
        NetOp::AShr => CVal::eval_binary(">>>", a(0).cast(w, true), a(1)).unwrap().cast(w, s),
        NetOp::Eq => CVal::eval_binary("==", a(0), a(1)).unwrap(),
        NetOp::Neq => CVal::eval_binary("!=", a(0), a(1)).unwrap(),
        NetOp::CaseEq => CVal::eval_binary("===", a(0), a(1)).unwrap(),
        NetOp::CaseNeq => CVal::eval_binary("!==", a(0), a(1)).unwrap(),
        NetOp::WildEq { rhs } => CVal::eval_binary("==?", a(0), rhs.clone()).unwrap(),
        NetOp::WildNeq { rhs } => CVal::eval_binary("!=?", a(0), rhs.clone()).unwrap(),
        NetOp::ULt => {
            let x = a(0);
            let y = a(1);
            CVal::eval_binary("<", x.cast(x.w, false), y.cast(y.w, false)).unwrap()
        }
        NetOp::SLt => {
            let x = a(0);
            let y = a(1);
            CVal::eval_binary("<", x.cast(x.w, true), y.cast(y.w, true)).unwrap()
        }
        NetOp::ULe => {
            let x = a(0);
            let y = a(1);
            CVal::eval_binary("<=", x.cast(x.w, false), y.cast(y.w, false)).unwrap()
        }
        NetOp::SLe => {
            let x = a(0);
            let y = a(1);
            CVal::eval_binary("<=", x.cast(x.w, true), y.cast(y.w, true)).unwrap()
        }
        NetOp::Mux => {
            let c = a(0);
            let tv = a(1);
            let ev = a(2);
            if c.known() {
                if c.is_true() {
                    tv
                } else {
                    ev
                }
            } else if tv == ev && tv.known() {
                tv
            } else {
                CVal::all_x(w, s)
            }
        }
        NetOp::Concat => {
            let mut acc: Option<CVal> = None;
            for &id in args {
                let p = vals[id.0 as usize].clone();
                acc = Some(match acc {
                    None => p,
                    Some(prev) => CVal::concat(&prev, &p),
                });
            }
            acc.unwrap().cast(w, s)
        }
        NetOp::Slice { hi, lo } => a(0).slice(*lo, (*hi - *lo) + 1, s),
        NetOp::ZeroExt { width } => {
            let x = a(0);
            x.cast(x.w, false).cast(*width, false).cast(*width, s)
        }
        NetOp::SignExt { width } => {
            let x = a(0);
            x.cast(x.w, true).cast(*width, true).cast(*width, s)
        }
        NetOp::CastFourState { four_state } => {
            let x = a(0);
            if *four_state {
                x
            } else {
                x.two_state()
            }
        }
        NetOp::CountOnes => {
            let x = a(0);
            let ones = (0..x.w).filter(|&i| x.bit_state(i) == BitState::One).count() as i128;
            CVal::new(32, true, ones)
        }
        NetOp::OneHot => {
            let x = a(0);
            let ones = (0..x.w).filter(|&i| x.bit_state(i) == BitState::One).count();
            CVal::bool(ones == 1)
        }
        NetOp::OneHot0 => {
            let x = a(0);
            let ones = (0..x.w).filter(|&i| x.bit_state(i) == BitState::One).count();
            CVal::bool(ones <= 1)
        }
        NetOp::IsUnknown => CVal::bool(!a(0).known()),
    }
}
