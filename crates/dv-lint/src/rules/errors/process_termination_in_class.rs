//! SV-ERR-003 (ProcessTerminationInClass): Flags $finish, $stop and $exit
//! called from class methods.

use crate::support::ast::{call_site, subtree, user_class_methods, CallSite};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

fn calls_in(c: &Cst, root: NodeId) -> Vec<CallSite> {
    subtree(c, root).into_iter().filter_map(|n| call_site(c, n)).collect()
}

pub(crate) fn termination_in_class(c: &Cst, s: &mut Sink) {
    for (_, class, methods) in user_class_methods(c) {
        for (m, name) in methods {
            let Some(body) = c.tree.child(m, Body) else { continue };
            for call in calls_in(c, body) {
                if !matches!(call.name.as_str(), "$finish" | "$stop" | "$exit") {
                    continue;
                }
                let Some(at) = c.first_tok(call.node) else { continue };
                s.add(
                    "SV-ERR-003",
                    "ProcessTerminationInClass",
                    Severity::Warning,
                    format!(
                        "{} in {class}::{name} ends the simulation without the check and report phases or the UVM \
                         summary, so regression scripts may read the log as a pass. Use `uvm_fatal \
                         to abort, or drop objections to end the test.",
                        call.name
                    ),
                    c.loc(at),
                );
            }
        }
    }
}
