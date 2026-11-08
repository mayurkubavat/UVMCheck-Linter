//! SVA-ACT-002 (TimeInAssertionAction): Action blocks of immediate and
//! concurrent assertions must not consume simulation time (# delays, @ event
//! controls, or wait statements) per IEEE 1800 § 16.

use crate::support::slang_ast::*;
use crate::support::procedural::start;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;
use crate::support::sva_cov::{actions, add_n, ast_asserts};

pub(crate) fn time_in_action(c: &Cst, s: &mut Sink) {
    for (kind, label) in [(AssertStmt, "immediate"), (ConcurrentAssert, "concurrent")] {
        for a in ast_asserts(c, kind) {
            let n = times(c, a);
            let (pass, fail) = actions(c, a);
            for act in [pass, fail].into_iter().flatten() {
                let mut last = None;
                for x in subtree(c, act) {
                    let timed = match c.kind(x) {
                        TimedStmt => true,
                        WaitStmt => c.keyword(x).is_some_and(|k| c.text_of(k) == "wait") && !c.toks(x).any(|t| c.text_of(t) == "fork"),
                        _ => false,
                    };
                    if timed {
                        last = start(c, x);
                    }
                }
                let Some(loc) = last else { continue };
                add_n(
                    s,
                    n,
                    "SVA-ACT-002",
                    "TimeInAssertionAction",
                    Severity::Error,
                    format!(
                        "Time-consuming statement (#delay, @event, wait) detected inside {label} assertion action block. In SystemVerilog (IEEE 1800 § 16), assertion action blocks execute in the Reactive region and must not consume simulation time."
                    ),
                    loc,
                );
            }
        }
    }
}
