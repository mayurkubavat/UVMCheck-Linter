//! SV-RND-014 (EmptyRandomizeWith): Flags 'randomize() with {}' containing an
//! empty inline constraint block.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

/// slang's `ArrayOrRandomizeMethodExpression` with a constraint block:
/// `x.randomize() with { ... }` is a call whose `WithClause` holds one.
pub(crate) fn empty_randomize_with(c: &Cst, s: &mut Sink) {
    for n in c.nodes(CallExpr) {
        let Some(w) = c.tree.child(n, WithClause) else { continue };
        let Some(block) = c.tree.child(w, ConstraintBlock) else { continue };
        let Some(method) = c.tree.children(n).next() else { continue };
        if method == w || !c.text(method).contains("randomize") {
            continue;
        }
        if c.tree.children(block).next().is_some() {
            continue;
        }
        let Some(with) = c.tok(w, "with") else { continue };
        s.add(
            "SV-RND-014",
            "EmptyRandomizeWith",
            Severity::Warning,
            "Inline constraint block 'with { }' is empty. \
             Empty inline constraints construct unnecessary dynamic solver scopes at runtime with zero constraints added. \
             Remove the redundant 'with { }' block."
                .into(),
            c.loc(with),
        );
    }
}
