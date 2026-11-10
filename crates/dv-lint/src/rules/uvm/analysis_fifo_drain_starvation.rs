//! UVM-TLM-006 (AnalysisFifoDrainStarvation): Detects instantiated
//! uvm_tlm_analysis_fifo components that receive transactions via connect_phase
//! but have zero drain operations (.get(), .try_get()) across all task-based
//! phases, causing unbounded simulation heap memory growth and fatal bad_alloc
//! crashes.

use crate::support::uvm_classes::{call_name, calls_in, is_component, Ix};
use crate::support::uvm_syntax::routine_last_name;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use std::collections::HashSet;
use svlang::syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn fifo_drain_starvation(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    let mut fifos = Vec::new();
    for cls in ix.model_classes() {
        if !is_component(d, cls.id) {
            continue;
        }
        for p in &cls.properties {
            if p.ty.contains("tlm_analysis_fifo") || p.ty.contains("tlm_fifo") {
                fifos.push((cls.name.as_str(), p.name.as_str(), p.loc));
            }
        }
    }
    if fifos.is_empty() {
        return;
    }
    let mut connected = HashSet::new();
    let mut drained = HashSet::new();
    for cls in ix.model_classes() {
        for at in ix.own_routines(cls.id) {
            let c = ix.cst(at);
            let is_connect = routine_last_name(c, at.node) == "connect_phase";
            let is_task = c.kind(at.node) == TaskDecl;
            for n in calls_in(c, at.node) {
                let name = call_name(c, n);
                if is_connect && name == "connect" {
                    let text = c.text(n);
                    for f in &fifos {
                        if text.contains(f.1) {
                            connected.insert(f.1);
                        }
                    }
                }
                if is_task && matches!(name, "get" | "try_get" | "peek" | "try_peek") {
                    let text = c.text(n);
                    for f in &fifos {
                        if text.contains(f.1) {
                            drained.insert(f.1);
                        }
                    }
                }
            }
        }
    }
    for (cls, name, loc) in fifos {
        if !connected.contains(name) || drained.contains(name) {
            continue;
        }
        s.add(
            "UVM-TLM-006",
            "AnalysisFifoDrainStarvation",
            Severity::Warning,
            format!(
                "Analysis FIFO '{name}' in component '{cls}' is connected to receive transactions in connect_phase, but is never drained (.get() / .try_get()) \
                 in any task phase across the verification environment. \
                 Unconsumed transactions will queue indefinitely, causing unbounded memory bloat."
            ),
            loc,
        );
    }
}
