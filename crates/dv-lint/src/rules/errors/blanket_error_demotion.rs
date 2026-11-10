//! UVM-REP-006 (BlanketErrorDemotion): Flags report catchers that demote or
//! swallow errors without filtering on ID or message, and severity
//! overrides/actions that disable UVM_ERROR or UVM_FATAL for every ID (Pylint
//! W0718 broad-exception-caught).

use crate::support::class_roles::{role, Index, Role};
use crate::support::routine_bodies::{bodies, calls_in};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use svlang::syntax::SyntaxKind::*;

fn is_error_severity(a: &str) -> bool {
    a.contains("UVM_ERROR") || a.contains("UVM_FATAL")
}

fn is_weak_severity(a: &str) -> bool {
    a.contains("UVM_INFO") || a.contains("UVM_WARNING")
}

/// An action that neither counts the error nor stops the run.
fn drops_action(a: &str) -> bool {
    a.contains("UVM_NO_ACTION") || (!a.contains("UVM_COUNT") && !a.contains("UVM_EXIT") && !a.contains("UVM_STOP"))
}

pub(crate) fn blanket_error_demotion(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for (cls, m, c, body) in bodies(d, &ix) {
        let calls = calls_in(c, body);
        let arg_text = |call, i: usize| {
            let args = c.call_args(call);
            args.get(i).and_then(|a| c.arg_expr(*a)).map(|e| c.expr_text(e))
        };
        if role(d, cls) == Role::Catcher && m.name == "catch" {
            let mut filters = false;
            let mut demotion = None;
            for &call in &calls {
                let n = c.callee_name(call);
                if matches!(n, "get_id" | "get_message" | "get_fname" | "get_context" | "get_client") {
                    filters = true;
                }
                if demotion.is_some() || c.call_args(call).is_empty() {
                    continue;
                }
                let Some(a) = arg_text(call, 0) else { continue };
                if (n == "set_severity" && is_weak_severity(&a)) || (n == "set_action" && drops_action(&a)) {
                    demotion = Some(call);
                }
            }
            let caught = c.subtree(body).into_iter().any(|r| {
                c.kind(r) == ReturnStmt && c.tree.children(r).next().is_some_and(|e| c.expr_text(e) == "CAUGHT")
            });
            if !filters && (demotion.is_some() || caught) {
                out.add(
                    "UVM-REP-006",
                    "BlanketErrorDemotion",
                    Severity::Error,
                    format!(
                        "Report catcher '{}' {} without checking get_id() or get_message(), so every matching error in the run is \
hidden, including ones nobody has seen yet. Filter on the specific ID or message.",
                        cls.name,
                        if demotion.is_some() { "demotes reports" } else { "returns CAUGHT" }
                    ),
                    demotion.map(|x| c.loc_of(x)).unwrap_or(m.loc),
                );
            }
        }
        for &call in &calls {
            let n = c.callee_name(call);
            let (Some(a0), Some(a1)) = (arg_text(call, 0), arg_text(call, 1)) else { continue };
            let bad = (n == "set_report_severity_override" && is_error_severity(&a0) && is_weak_severity(&a1))
                || (matches!(n, "set_report_severity_action" | "set_report_severity_action_hier")
                    && is_error_severity(&a0)
                    && drops_action(&a1));
            if bad {
                out.add(
                    "UVM-REP-006",
                    "BlanketErrorDemotion",
                    Severity::Error,
                    format!(
                        "{n}({a0}, {a1}) changes how every {a0} is handled, for all report IDs. Errors stop counting and the test can pass \
with real failures. Use the *_id_* variant for the specific ID instead."
                    ),
                    c.loc_of(call),
                );
            }
        }
    }
}
