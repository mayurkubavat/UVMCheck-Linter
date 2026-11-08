//! SV-RND-025 (DistWideRangeColonEq): Flags '[lo:hi] := w' in a dist when the
//! range holds more than max_range_values (default 16) values and the dist also
//! has items of another size. ':=' applies the weight to each value, which is
//! rarely what the author meant; ':/' shares it.

use crate::support::class_items::{expression_constraint, kids, subtree};
use crate::support::constraints::{const_int, constraint_blocks, param_values, user_model_classes};
use crate::support::data_items::unparen;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::HashMap;
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

/// Number of values `[lo:hi]` covers.
fn range_width(c: &Cst, value: NodeId, params: &HashMap<String, NodeId>) -> Option<i64> {
    if c.kind(value) != OpenRange || c.tok(value, ":").is_none() {
        return None;
    }
    let k = kids(c, value);
    let [lo, hi] = k.as_slice() else { return None };
    let (lo, hi) = (const_int(c, *lo, params, 0)?, const_int(c, *hi, params, 0)?);
    Some((hi - lo).abs() + 1)
}

pub(crate) fn dist_wide_range(c: &Cst, s: &mut Sink) {
    let params = param_values(c);
    for (cls, _) in user_model_classes(c) {
        for block in constraint_blocks(c, cls) {
            for item in subtree(c, block).into_iter().filter(|&x| c.kind(x) == ConstraintItem) {
                let Some((_, e)) = expression_constraint(c, item) else { continue };
                let e = unparen(c, e);
                if c.kind(e) != ExprOrDist || c.tok(e, "dist").is_none() {
                    continue;
                }
                let Some(list) = c.tree.child(e, RangeList) else { continue };
                let items: Vec<NodeId> = c.tree.children(list).filter(|&x| c.kind(x) == DistItem).collect();
                let widths: Vec<Option<i64>> = items
                    .iter()
                    .map(|&it| c.tree.children(it).next().and_then(|v| range_width(c, v, &params)))
                    .collect();
                if widths.iter().all(|w| w.is_some() && *w == widths[0]) {
                    continue;
                }
                for (&it, w) in items.iter().zip(&widths) {
                    let Some(w) = *w else { continue };
                    if w <= 16 || c.tok(it, ":=").is_none() {
                        continue;
                    }
                    let Some(at) = c.tree.children(it).next().and_then(|v| c.first_tok(v)) else { continue };
                    s.add(
                        "SV-RND-025",
                        "DistWideRangeColonEq",
                        Severity::Warning,
                        format!(
                            "':=' gives every one of the {w} values in this range the full weight, so the range \
                             outweighs the single values next to it. Use ':/' to share the weight across the range, \
                             or write the per-value weight on purpose."
                        ),
                        c.loc(at),
                    );
                }
            }
        }
    }
}
