//! UVM-OBS-002 (BlockingInObserver): Analysis write() implementations
//! (uvm_subscriber::write and the `write<SFX>` methods behind
//! `uvm_analysis_imp<SFX>`) must be functions that finish in zero time.

use crate::support::uvm_classes::{class_role, is_join_none, user_classes, Ix, Role};
use crate::support::uvm_syntax::{body_items, name_node, routine_last_name, subtree};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use svlang::syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

pub(crate) fn blocking_in_observer(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for cls in user_classes(d) {
        let mut writes: Vec<String> = Vec::new();
        if class_role(d, cls.id) == Role::Subscriber {
            writes.push("write".into());
        }
        for p in &cls.properties {
            let Some(t) = p.type_class else { continue };
            let n = &d.class(t).name;
            if let Some(sfx) = n.strip_prefix("uvm_analysis_imp") {
                writes.push(format!("write{sfx}"));
            }
        }
        if writes.is_empty() {
            continue;
        }
        for at in ix.all_routines(cls.id) {
            let c = ix.cst(at);
            let name = routine_last_name(c, at.node);
            if !writes.contains(&name) {
                continue;
            }
            let wh = format!("'{name}' of class '{}'", cls.name);
            if c.kind(at.node) == TaskDecl {
                let Some(loc) =
                    name_node(c, at.node).and_then(|x| c.tree.token_range(x).last().copied()).map(|t| c.loc(t))
                else {
                    continue;
                };
                s.add(
                    "UVM-OBS-002",
                    "BlockingInObserver",
                    Severity::Error,
                    format!(
                        "Analysis method {wh} is declared as a task. Analysis ports call \
                         write() as a function in zero time; make it a void function and move \
                         time-consuming work to run_phase() behind a uvm_tlm_analysis_fifo."
                    ),
                    loc,
                );
                continue;
            }
            let found = body_items(c, at.node).into_iter().find_map(|i| first_join_none(c, i));
            let Some(fork) = found else { continue };
            let Some(t) = c.first_tok(fork) else { continue };
            s.add(
                "UVM-OBS-002",
                "BlockingInObserver",
                Severity::Warning,
                format!(
                    "Analysis method {wh} forks a thread for every transaction \
                     (fork ... join_none). Threads pile up under load and transactions lose \
                     their order. Push the transaction into a uvm_tlm_analysis_fifo and \
                     process it from one run_phase() thread."
                ),
                c.loc(t),
            );
        }
    }
}

fn first_join_none(c: &Cst, n: NodeId) -> Option<NodeId> {
    subtree(c, n).into_iter().find(|&x| is_join_none(c, x))
}
