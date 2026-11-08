//! SV-COV-008 (CrossWithEmptyBins): Detects degenerate covergroup cross
//! constructs specifying fewer than 2 coverpoints or empty cross products.

use crate::support::names::cross_parts;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn cross_with_empty_bins(c: &Cst, s: &mut Sink) {
    for n in c.nodes(Cross) {
        let (label, items) = cross_parts(c, n);
        if items.len() >= 2 {
            continue;
        }
        let Some(kw) = c.tok(n, "cross") else { continue };
        let label = label.unwrap_or_else(|| "unnamed_cross".into());
        s.add(
            "SV-COV-008",
            "CrossWithEmptyBins",
            Severity::Warning,
            format!(
                "Covergroup cross '{label}' specifies fewer than 2 coverpoints. Cross coverage requires at least two distinct coverpoints to construct a Cartesian product."
            ),
            c.loc(kw),
        );
    }
}
