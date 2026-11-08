//! SV-COV-010 (CrossWithAllBinsIgnored): Flags covergroup cross constructs
//! where all defined bins are ignore_bins, producing vacuous 0-bin coverage
//! metrics.

use crate::support::cross_labels::{cross_parts, label};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

pub(crate) fn cross_all_ignored(c: &Cst, s: &mut Sink) {
    for n in c.nodes(Cross) {
        let (_, members, kw) = cross_parts(c, n);
        let bins: Vec<NodeId> = members.into_iter().filter(|&m| c.kind(m) == Bins).collect();
        let ignored = bins.iter().filter(|&&b| c.keyword(b).is_some_and(|k| c.text_of(k) == "ignore_bins")).count();
        if bins.is_empty() || ignored != bins.len() {
            continue;
        }
        let Some(kw) = kw else { continue };
        let name = label(c, n).unwrap_or_else(|| "anonymous_cross".into());
        s.add(
            "SV-COV-010",
            "CrossWithAllBinsIgnored",
            Severity::Warning,
            format!(
                "Covergroup cross '{name}' defines only 'ignore_bins' with zero positive coverage bins. Masking all cross combinations produces a degenerate 0-bin cross that cannot collect coverage closure data."
            ),
            c.loc(kw),
        );
    }
}
