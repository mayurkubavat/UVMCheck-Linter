//! SV-DLY-004 (ZeroTimeForeverLoop): Flags forever loops whose body has no
//! timing control, no task call and no exit, so the loop spins in zero
//! simulation time and hangs the simulator.

use crate::support::procedural::{add_n, is_task_call, loop_body, loop_kw, start};
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn zero_time_forever(c: &Cst, s: &mut Sink) {
    for l in c.nodes(LoopStmt) {
        if loop_kw(c, l) != "forever" {
            continue;
        }
        let Some(body) = loop_body(c, l) else { continue };
        let found = subtree(c, body).into_iter().any(|x| match c.kind(x) {
            TimedStmt | WaitStmt | DisableStmt | ReturnStmt => true,
            JumpStmt => c.keyword(x).is_some_and(|k| c.text_of(k) == "break"),
            CallExpr => {
                let name = call_parts(c, x).map(|p| p.1).unwrap_or_default();
                if name.starts_with('$') {
                    matches!(name.as_str(), "$finish" | "$stop" | "$fatal")
                } else {
                    is_task_call(c, x)
                }
            }
            _ => false,
        });
        if found {
            continue;
        }
        if let Some(loc) = start(c, l) {
            add_n(
                s,
                times(c, l),
                "SV-DLY-004",
                "ZeroTimeForeverLoop",
                Severity::Error,
                "forever loop with no timing control (#, @, wait), no task call and no way out (break, \
                 return, disable). It runs forever in zero simulation time and hangs the simulator. \
                 Wait on a clock or event inside the loop, or call the blocking task it is meant to wait in."
                    .into(),
                loc,
            );
        }
    }
}
