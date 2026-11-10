//! SVA-ACT-003 (SideEffectInAssertionAction): Flags assignments, increments and
//! other writes inside the pass or fail action of an immediate or concurrent
//! assertion.

use crate::support::procedural::{start, written};
use crate::support::slang_ast::*;
use crate::support::sva_cov::{actions, add_n, ast_asserts, user_code_times};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::HashSet;
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

pub(crate) fn side_effect_in_action(c: &Cst, s: &mut Sink) {
    for kind in [AssertStmt, ConcurrentAssert] {
        for a in ast_asserts(c, kind) {
            let n = user_code_times(c, a);
            if n == 0 {
                continue;
            }
            let (pass, fail) = actions(c, a);
            for act in [pass, fail].into_iter().flatten() {
                let inside: HashSet<NodeId> = subtree(c, act).into_iter().collect();
                let mut names: Vec<String> = written(c, act, true)
                    .into_iter()
                    .filter(|w| {
                        // A variable declared in a block of the action itself.
                        !w.2.is_some_and(|d| {
                            d.kind == crate::support::slang_ast::DeclKind::Local
                                && inside.contains(&d.node)
                                && c.parent(d.node)
                                    .and_then(|p| c.parent(p))
                                    .is_some_and(|b| matches!(c.kind(b), SeqBlock | ParBlock))
                        })
                    })
                    .map(|w| w.1)
                    .collect();
                if names.is_empty() {
                    continue;
                }
                names.sort();
                let list = names.iter().map(|x| format!("'{x}'")).collect::<Vec<_>>().join(", ");
                let Some(loc) = start(c, act) else { continue };
                add_n(
                    s,
                    n,
                    "SVA-ACT-003",
                    "SideEffectInAssertionAction",
                    Severity::Warning,
                    format!(
                        "Assertion action block changes {list}. When assertions are turned off the action never runs, so the testbench behaves \
                         differently. Keep action blocks to reporting, and update state outside the assertion."
                    ),
                    loc,
                );
            }
        }
    }
}
