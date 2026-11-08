//! SV-DLY-003 (RepeatLoopWithoutTiming): Flags repeat loops inside testbench
//! tasks that contain no time-consuming statements (#, @, wait).

use crate::support::procedural::{loop_body, loop_kw, routine_name};
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn repeat_without_timing(c: &Cst, s: &mut Sink) {
    for t in c.nodes(TaskDecl) {
        let name = routine_name(c, t);
        for l in subtree(c, t) {
            if c.kind(l) != LoopStmt || loop_kw(c, l) != "repeat" {
                continue;
            }
            let Some(body) = loop_body(c, l) else { continue };
            let timed = subtree(c, body).into_iter().any(|x| match c.kind(x) {
                TimedStmt | ParBlock => true,
                WaitStmt => {
                    c.keyword(x).is_some_and(|k| c.text_of(k) == "wait") && !c.toks(x).any(|t| c.text_of(t) == "fork")
                }
                _ => false,
            });
            if timed {
                continue;
            }
            let text = slang_text(c, body);
            if text.contains('@') || text.contains('#') || text.contains("wait") {
                continue;
            }
            let Some(kw) = c.tok(l, "repeat") else { continue };
            s.add(
                "SV-DLY-003",
                "RepeatLoopWithoutTiming",
                Severity::Warning,
                format!(
                    "Repeat loop inside task '{name}()' contains no time-consuming statements (#, @, wait). \
                     In verification testbenches, tasks operate across simulation time; \
                     a repeat loop without clock or delay synchronization spins instantaneously in a single delta cycle and frequently indicates a missing '@(posedge clk)' or clocking block synchronization."
                ),
                c.loc(kw),
            );
        }
    }
}
