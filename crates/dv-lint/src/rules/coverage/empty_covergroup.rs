//! SV-COV-007 (EmptyCovergroup): Flags covergroups declared with zero
//! coverpoints and zero crosses.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;
use crate::support::names::{name_tok};

pub(crate) fn empty_covergroup(c: &Cst, s: &mut Sink) {
    for n in c.nodes(CovergroupDecl) {
        if c.tree.children(n).any(|x| matches!(c.kind(x), Coverpoint | Cross)) {
            continue;
        }
        let name = name_tok(c, n);
        let cg = name.map(|t| c.text_of(t)).filter(|t| !t.is_empty()).unwrap_or("unnamed_covergroup");
        let Some(at) = name.or_else(|| c.first_tok(n)) else { continue };
        s.add(
            "SV-COV-007",
            "EmptyCovergroup",
            Severity::Warning,
            format!(
                "Covergroup '{cg}' is declared with zero coverpoints or crosses. \
                 Empty covergroups collect zero verification metrics and provide a false impression of coverage closure. \
                 Define coverpoints or crosses inside the covergroup, or remove the unused declaration."
            ),
            c.loc(at),
        );
    }
}
