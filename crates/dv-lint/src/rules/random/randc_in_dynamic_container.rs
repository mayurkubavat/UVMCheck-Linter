//! SV-RND-020 (RandcInDynamicContainer): Disallows declaring 'randc' cyclic
//! variables on dynamic arrays, queues, or associative arrays (IEEE 1800 §
//! 18.4.1).

use crate::support::class_items::{class_items, declarators, kids, model_classes, own, qualifiers};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

/// A dynamic array, queue or associative array dimension: `[]`, `[$]`,
/// `[$:N]`, `[*]`, `[type]`.
pub(crate) fn is_dynamic_dim(c: &Cst, d: NodeId) -> bool {
    let o = own(c, d);
    if o == ["[", "]"] && c.tree.children(d).next().is_none() {
        return true;
    }
    if o.contains(&"$") || o.contains(&"*") {
        return o.first() == Some(&"[") && o.get(1).is_some_and(|x| *x == "$" || *x == "*");
    }
    // `[string]`, `[int]`, `[my_t]`: an associative index type.
    let k = kids(c, d);
    k.len() == 1 && matches!(c.kind(k[0]), DataType | TypeExpr)
}

pub(crate) fn randc_dynamic(c: &Cst, s: &mut Sink) {
    for (cls, _) in model_classes(c) {
        for it in class_items(c, cls) {
            if c.kind(it) != DataDecl || !qualifiers(c, it).contains(&"randc") {
                continue;
            }
            for (d, n, t) in declarators(c, it) {
                if !c.tree.children(d).any(|x| c.kind(x) == Dim && is_dynamic_dim(c, x)) {
                    continue;
                }
                s.add(
                    "SV-RND-020",
                    "RandcInDynamicContainer",
                    Severity::Error,
                    format!(
                        "Class property '{n}' is declared as 'randc' on a dynamic container. \
                         In SystemVerilog (IEEE 1800 § 18.4.1), 'randc' cyclic variables \
                         are strictly prohibited on dynamic arrays, queues, and associative arrays."
                    ),
                    c.loc(t),
                );
            }
        }
    }
}
