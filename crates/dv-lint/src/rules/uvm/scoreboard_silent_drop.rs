//! UVM-SCB-001 (ScoreboardSilentDrop): Detects silent returns in scoreboard
//! write methods when expected queues are empty without logging an error.

use crate::support::uvm_classes::{if_parts, is_scoreboard, Ix};
use crate::support::uvm_syntax::{routine_last_name, subtree};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use svlang::syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn scoreboard_silent_drop(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for cls in ix.model_classes() {
        if !is_scoreboard(d, cls) {
            continue;
        }
        for at in ix.own_routines(cls.id) {
            let c = ix.cst(at);
            let method = routine_last_name(c, at.node);
            if !method.starts_with("write") {
                continue;
            }
            for n in subtree(c, at.node).into_iter().filter(|&n| c.kind(n) == IfStmt) {
                let Some((cond, then, _)) = if_parts(c, n) else { continue };
                let cs = c.text(cond);
                if !(cs.contains(".size()") || cs.contains(".num()") || cs.contains(".empty()")) {
                    continue;
                }
                let body = c.text(then);
                if !body.contains("return") {
                    continue;
                }
                let reported = ["uvm_error", "uvm_fatal", "assert", "error", "fatal", "drop_count"]
                    .iter()
                    .any(|k| body.contains(k));
                if reported {
                    continue;
                }
                let Some(t) = c.first_tok(n) else { continue };
                s.add(
                    "UVM-SCB-001",
                    "ScoreboardSilentDrop",
                    Severity::Error,
                    format!(
                        "Analysis export method '{method}' in scoreboard class '{}' silently returns when expected queue is empty ('{cs}') without logging an error. If the DUT generates spurious, \
                         misrouted, or unpredicted packets, this silent drop masks the hardware bug. \
                         Report an error with '`uvm_error(\"SPURIOUS_PKT\", ...)' before returning.",
                        cls.name
                    ),
                    c.loc(t),
                );
            }
        }
    }
}
