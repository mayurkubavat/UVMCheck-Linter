//! SV-RND-004 (InlineShadowNoLocal): Detects inline constraint variable
//! shadowing inside 'randomize() with' missing 'local::'.

use crate::support::class_items::{kids, op, subtree, trim, with_clause};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind::*;

pub(crate) fn inline_shadow_no_local(c: &Cst, s: &mut Sink) {
    for call in c.nodes(CallExpr) {
        let Some(w) = with_clause(c, call) else { continue };
        let Some(block) = c.tree.child(w, ConstraintBlock) else { continue };
        let Some(method) = c.tree.children(call).next() else { continue };
        let method_str = c.text(method);
        if !method_str.contains("randomize") {
            continue;
        }
        let target = method_str.find('.').map(|i| trim(&method_str[..i]).to_string()).unwrap_or_default();
        for n in subtree(c, block) {
            if c.kind(n) != BinaryExpr || !matches!(op(c, n), "==" | "==?" | "===") {
                continue;
            }
            let k = kids(c, n);
            let (Some(&l), Some(&r)) = (k.first(), k.get(1)) else { continue };
            let (ls, rs) = (c.trimmed(l), c.trimmed(r));
            if ls.is_empty() || ls != rs || ls.contains("local::") || ls.contains('.') {
                continue;
            }
            let prefix = if target.is_empty() { "item" } else { &target };
            let Some(at) = c.first_tok(n) else { continue };
            s.add(
                "SV-RND-004",
                "InlineShadowNoLocal",
                Severity::Error,
                format!(
                    "Inline constraint '{ls} == {rs}' in 'randomize() with' shadows \
                     the local scope variable without the 'local::' qualifier (IEEE 1800 § 18.7). Object properties \
                     take precedence in inline constraints, causing '{ls} == {rs}' to evaluate \
                     as a tautology on '{prefix}.{ls}' and leaving the field unconstrained. \
                     Qualify the local variable with 'local::{ls}'."
                ),
                c.loc(at),
            );
        }
    }
}
