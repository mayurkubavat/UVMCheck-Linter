//! SV-COV-003 (CrossSameCoverpoint): Detects coverage cross constructs crossing
//! a coverpoint with itself (cross cp_a, cp_a).

use crate::support::names::cross_parts;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn cross_same_coverpoint(c: &Cst, s: &mut Sink) {
    for n in c.nodes(Cross) {
        let (label, items) = cross_parts(c, n);
        let mut seen = std::collections::HashSet::new();
        for item in items {
            let name = c.trimmed(item);
            if name.is_empty() || seen.insert(name.clone()) {
                continue;
            }
            let Some(first) = c.first_tok(item) else { continue };
            let cross = label.clone().unwrap_or_else(|| "unnamed".into());
            s.add(
                "SV-COV-003",
                "CrossSameCoverpoint",
                Severity::Error,
                format!(
                    "Coverage cross '{cross}' crosses coverpoint '{name}' with itself. Crossing identical coverpoints creates redundant diagonal bins and skews regression coverage metrics."
                ),
                c.loc(first),
            );
        }
    }
}
