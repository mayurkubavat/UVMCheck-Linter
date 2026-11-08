//! SV-CON-009 (WaitForkWithoutFork): Flags 'wait fork;' statements in tasks or
//! routines that contain no 'fork' blocks.

use sv_syntax::SyntaxKind;
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::procedural::{routine_name, start};

pub(crate) fn wait_fork_without_fork(c: &Cst, s: &mut Sink) {
    for k in [FunctionDecl, TaskDecl] {
        for r in c.nodes(k) {
            let name = routine_name(c, r);
            let all = subtree(c, r);
            if all.iter().any(|&x| c.kind(x) == ParBlock) {
                continue;
            }
            for x in all {
                if c.kind(x) == WaitStmt && c.toks(x).any(|t| c.text_of(t) == "fork") {
                    if let Some(loc) = start(c, x) {
                        s.add(
                            "SV-CON-009",
                            "WaitForkWithoutFork",
                            Severity::Warning,
                            format!(
                                "Routine '{name}' executes 'wait fork;' but contains no parallel 'fork' blocks. \
                                 In SystemVerilog (IEEE 1800 § 9.6.1), 'wait fork' blocks execution until all immediate child processes \
                                 spawned by the current process terminate. Calling 'wait fork' in a routine that spawned zero child threads \
                                 either executes as a useless no-op or causes unintended hangs by waiting on unrelated threads spawned by caller processes."
                            ),
                            loc,
                        );
                    }
                }
            }
        }
    }
}
