//! SV-COV-015 (MissingIffOnCovergroupClock): Covergroups sampled by a clocking
//! event should specify an 'iff' reset guard condition.

use crate::support::names::name_tok;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn missing_iff(c: &Cst, s: &mut Sink) {
    for n in c.nodes(CovergroupDecl) {
        let Some(ev) = c.tree.children(n).find(|&x| matches!(c.kind(x), TimingControl | BlockEvent | WithClause))
        else {
            continue;
        };
        let text = c.text(ev);
        if !text.contains('@') || text.contains("iff") {
            continue;
        }
        let Some(first) = c.first_tok(ev) else { continue };
        let cg = name_tok(c, n).map(|t| c.text_of(t)).unwrap_or("");
        s.add(
            "SV-COV-015",
            "MissingIffOnCovergroupClock",
            Severity::Warning,
            format!(
                "Covergroup '{cg}' is sampled by clocking event '{text}' without an 'iff (!reset)' guard condition. In functional coverage methodology \
                 (IEEE 1800 § 19.4), clock-sampled covergroups should include an 'iff' \
                 guard to disable sampling during active reset, preventing reset-state transactions from contaminating closure metrics."
            ),
            c.loc(first),
        );
    }
}
