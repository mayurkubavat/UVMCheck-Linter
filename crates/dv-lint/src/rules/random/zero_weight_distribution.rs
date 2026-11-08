//! SV-RND-012 (ZeroWeightDistribution): Detects constraint distribution blocks
//! where all weights evaluate to zero.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;
use crate::support::spans::{child_nodes};

pub(crate) fn zero_weight_dist(c: &Cst, s: &mut Sink) {
    for n in c.nodes(ExprOrDist) {
        let Some(dist) = c.tok(n, "dist") else { continue };
        let Some(list) = c.tree.child(n, RangeList) else { continue };
        let items = child_nodes(c, list);
        let all_zero = items.iter().all(|&item| {
            if c.kind(item) != DistItem || (c.tok(item, ":=").is_none() && c.tok(item, ":/").is_none()) {
                return false;
            }
            let Some(&w) = child_nodes(c, item).last() else { return false };
            matches!(c.trimmed(w).as_str(), "0" | "'0" | "32'd0" | "32'b0")
        });
        if !items.is_empty() && all_zero {
            s.add(
                "SV-RND-012",
                "ZeroWeightDistribution",
                Severity::Error,
                "All weights in constraint distribution ('dist { ... }') evaluate to zero. \
                 Per IEEE 1800 § 18.5.4, at least one weight in a distribution constraint must evaluate to a non-zero value; a distribution sum of zero creates an unsatisfiable constraint that triggers solver runtime failure."
                    .to_string(),
                c.loc(dist),
            );
        }
    }
}
