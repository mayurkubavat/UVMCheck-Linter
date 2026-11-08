//! UVM-PHS-007 (UvmPhaseJumpHazard): Disallows manual phase jumping
//! ('phase.jump(...)') in UVM testbenches due to objection leaks and scoreboard
//! desynchronization risks.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn phase_jump(c: &Cst, s: &mut Sink) {
    for n in c.nodes(CallExpr) {
        let Some(left) = c.tree.children(n).next() else { continue };
        if c.last_text(left) != "jump" {
            continue;
        }
        let text = c.text(left);
        if !text.contains("phase") && !text.contains("domain") {
            continue;
        }
        let Some(first) = c.first_tok(left) else { continue };
        s.add(
            "UVM-PHS-007",
            "UvmPhaseJumpHazard",
            Severity::Warning,
            "Manual UVM phase jump via 'phase.jump()' abruptly interrupts the UVM phasing schedule. \
             Dynamic phase jumping causes phase objection leaks, scoreboard desynchronization, and dangling TLM transactions. \
             Coordinate testbench phases using standard sequences, events, or reset callbacks instead."
                .into(),
            c.loc(first),
        );
    }
}
