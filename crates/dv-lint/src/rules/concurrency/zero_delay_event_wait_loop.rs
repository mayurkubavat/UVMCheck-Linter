//! SV-EVT-001 (ZeroDelayEventWaitLoop): Using wait(event.triggered) inside a
//! loop without an intervening time-advancing delay (@ or #) causes a
//! zero-delay infinite delta loop that hangs the simulator at 100% CPU.

use crate::support::procedural::{add_n, is_wait_loop, loop_body, start};
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn triggered_wait_loop(c: &Cst, s: &mut Sink) {
    for l in subtree(c, c.tree.root) {
        if !is_wait_loop(c, l) {
            continue;
        }
        let Some(body) = loop_body(c, l) else { continue };
        let mut advance = false;
        let mut wait_loc = None;
        for x in subtree(c, body) {
            match c.kind(x) {
                TimedStmt => advance = true,
                WaitStmt
                    if c.keyword(x).is_some_and(|k| c.text_of(k) == "wait")
                        && !c.toks(x).any(|t| c.text_of(t) == "fork") =>
                {
                    let cond = kids(c, x).into_iter().find(|&y| !matches!(c.kind(y), StmtLabel | Attribute));
                    if cond.is_some_and(|cd| slang_text(c, cd).contains(".triggered")) {
                        wait_loc = start(c, x);
                    }
                }
                _ => {}
            }
        }
        let Some(loc) = wait_loc else { continue };
        if advance {
            continue;
        }
        add_n(
            s,
            times(c, l),
            "SV-EVT-001",
            "ZeroDelayEventWaitLoop",
            Severity::Error,
            "Loop contains 'wait(event.triggered)' without an intervening simulation time advance (@ or #). \
             In SystemVerilog (IEEE 1800 Sec 15.5.2), 'event.triggered' remains high for the entire current time-step across all delta cycles. \
             Without a time-advancing delay, this loop executes infinitely in the same delta cycle, deadlocking the simulator."
                .into(),
            loc,
        );
    }
}
