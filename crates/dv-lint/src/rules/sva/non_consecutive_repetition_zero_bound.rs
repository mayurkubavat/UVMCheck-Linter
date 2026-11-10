//! SV-SVA-004 (NonConsecutiveRepetitionZeroBound): Non-consecutive repetition
//! operators ([= ...] and [-> ...]) must specify a minimum bound >= 1.

use crate::support::spans::span;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind::*;

pub(crate) fn zero_repetition(c: &Cst, s: &mut Sink) {
    for n in c.nodes(Repetition) {
        let toks: Vec<u32> = c.toks(n).collect();
        let Some(&op) = toks.get(1) else { continue };
        let op_text = c.text_of(op);
        if op_text != "->" && op_text != "=" {
            continue;
        }
        let Some((_, last)) = c.tree.token_bounds(n) else { continue };
        if last <= op + 1 {
            continue;
        }
        let sel: String = span(c, op + 1, last - 1).chars().filter(|ch| !ch.is_ascii_whitespace()).collect();
        let zero = sel == "0" || sel == "1'b0" || sel == "32'd0" || sel.starts_with("0:") || sel.starts_with("1'b0:");
        if zero {
            s.add(
                "SV-SVA-004",
                "NonConsecutiveRepetitionZeroBound",
                Severity::Error,
                format!(
                    "Non-consecutive repetition operator '[{op_text} {sel}]' specifies a zero-repetition bound. Under IEEE 1800 § 16.9.2 assertion semantics, \
                     non-consecutive repetitions ('[= ...]' and '[-> ...]') must specify a positive minimum repetition bound (>= 1). \
                     Repetition bounds of 0 are vacuous, illegal under standard LRM rules, and cause assertion solver aborts."
                ),
                c.loc(op),
            );
        }
    }
}
