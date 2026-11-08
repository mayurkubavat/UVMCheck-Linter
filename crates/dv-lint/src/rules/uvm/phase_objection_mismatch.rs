//! UVM-PHS-006 (PhaseObjectionMismatch): Detects 'phase.raise_objection()'
//! calls without a matching 'phase.drop_objection()' in the same task or
//! method.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;
use crate::support::uvm_syntax::{body_items, callee, routine_last_name, routines, subtree};

pub(crate) fn phase_objection_mismatch(c: &Cst, s: &mut Sink) {
    for f in routines(c) {
        let (mut raise, mut drop, mut first) = (0usize, 0usize, None);
        for item in body_items(c, f) {
            for n in subtree(c, item) {
                if c.kind(n) != CallExpr {
                    continue;
                }
                match callee(c, n) {
                    "raise_objection" => {
                        raise += 1;
                        if first.is_none() {
                            first = c.first_tok(n);
                        }
                    }
                    "drop_objection" => drop += 1,
                    _ => {}
                }
            }
        }
        if raise > drop {
            let Some(at) = first else { continue };
            s.add(
                "UVM-PHS-006",
                "PhaseObjectionMismatch",
                Severity::Error,
                format!(
                    "UVM phase objection raised via 'raise_objection()' in '{}()' without matching 'drop_objection()'. Unpaired phase objections prevent phase completion and cause simulation to hang indefinitely.",
                    routine_last_name(c, f)
                ),
                c.loc(at),
            );
        }
    }
}
