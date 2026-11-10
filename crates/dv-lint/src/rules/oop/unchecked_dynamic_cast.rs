//! SV-OOP-002 (UncheckedDynamicCast): The return value of $cast(dest, src) must
//! be checked (e.g., in an if statement or assert) to prevent silent
//! null-pointer dereferences on cast failures.

use crate::support::literals::ast_times;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn unchecked_dynamic_cast(c: &Cst, s: &mut Sink) {
    for st in c.nodes(ExprStmt) {
        let Some(mut e) = c.tree.children(st).next() else { continue };
        // `void'($cast(a, b))`
        if c.kind(e) == CastExpr && c.tree.children(e).next().is_some_and(|t| c.tree.bare_text(t) == "void") {
            let Some(x) = c.tree.children(e).nth(1) else { continue };
            e = x;
        }
        if c.kind(e) != CallExpr || crate::support::class_items::call_name(c, e) != Some("$cast") {
            continue;
        }
        for _ in 0..ast_times(c, e) {
            s.add(
                "SV-OOP-002",
                "UncheckedDynamicCast",
                Severity::Warning,
                "Return value of $cast() is ignored or cast to void. If dynamic cast fails, destination handle remains null, risking runtime null-pointer dereferences."
                    .to_string(),
                c.loc_of(st),
            );
        }
    }
}
