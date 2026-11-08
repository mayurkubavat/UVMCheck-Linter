//! SV-THR-003 (UnboundedThreadSpawn): Flags fork ... join_none inside forever
//! or while loops with no wait fork, disable fork or semaphore get in the loop
//! to bound outstanding threads (thread-leak checks).

use crate::support::procedural::{
    add_n, is_semaphore_get, is_wait_loop, join_kind, loop_body, loop_kw, routine_name, start,
};
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

pub(crate) fn unbounded_spawn(c: &Cst, s: &mut Sink) {
    for (r, n) in user_routines(c) {
        let where_ = format!("'{}'", routine_name(c, r));
        for l in subtree(c, r) {
            if !is_wait_loop(c, l) {
                continue;
            }
            let kind = if c.kind(l) == DoWhileStmt { "do-while" } else { loop_kw(c, l) };
            let Some(body) = loop_body(c, l) else { continue };
            let mut spawn = None;
            let mut bounded = false;
            let mut stack = vec![body];
            while let Some(x) = stack.pop() {
                match c.kind(x) {
                    ParBlock if join_kind(c, x) == "join_none" => {
                        if spawn.is_none() {
                            spawn = Some(x);
                        }
                        continue;
                    }
                    WaitStmt if c.toks(x).any(|t| c.text_of(t) == "fork") => bounded = true,
                    DisableStmt if c.toks(x).nth(1).is_some_and(|t| c.text_of(t) == "fork") => bounded = true,
                    CallExpr if is_semaphore_get(c, x) => bounded = true,
                    _ => {}
                }
                let k: Vec<NodeId> = c.tree.children(x).collect();
                stack.extend(k.into_iter().rev());
            }
            let Some(f) = spawn else { continue };
            if bounded {
                continue;
            }
            let Some(loc) = start(c, f) else { continue };
            add_n(
                s,
                n,
                "SV-THR-003",
                "UnboundedThreadSpawn",
                Severity::Warning,
                format!(
                    "fork ... join_none inside a {kind} loop in {where_} starts a new thread every iteration with nothing that limits how many run at once. \
                     Threads pile up when the consumer is slow and are still running at end of test; bound \
                     them with a semaphore, or wait fork before the next batch."
                ),
                loc,
            );
        }
    }
}
