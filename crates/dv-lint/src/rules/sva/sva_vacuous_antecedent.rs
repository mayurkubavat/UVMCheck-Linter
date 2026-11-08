//! SVA-VAC-001 (SvaVacuousAntecedent): Detects constant-false antecedents in
//! assertion implications (0 |-> ...).

use crate::support::cross_labels::kids;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn vacuous_antecedent(c: &Cst, s: &mut Sink) {
    for n in c.nodes(SequenceExpr) {
        if c.tok(n, "|->").is_none() && c.tok(n, "|=>").is_none() {
            continue;
        }
        let Some(&left) = kids(c, n).first() else { continue };
        let ant = c.trimmed(left);
        let vacuous = matches!(ant.as_str(), "0" | "1'b0" | "'0" | "false" | "1'sb0")
            || ant.contains("&& !")
            || ant.contains("&& ~");
        let Some(t) = c.first_tok(left).filter(|_| vacuous) else { continue };
        s.add(
            "SVA-VAC-001",
            "SvaVacuousAntecedent",
            Severity::Error,
            format!(
                "Vacuous assertion implication detected: antecedent '{ant}' evaluates to constant false. \
                 In SVA (IEEE 1800 § 16.12.22), a false antecedent makes the implication evaluate to true on every cycle, \
                 causing the assertion to pass vacuously 100% of the time and masking unverified DUT logic."
            ),
            c.loc(t),
        );
    }
}
