//! SV-RND-009 (StaticConstraint): Disallows static constraints.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn static_constraint(c: &Cst, s: &mut Sink) {
    for n in c.nodes(ConstraintDecl) {
        let Some(st) = c.toks(n).take_while(|&t| c.text_of(t) != "constraint").find(|&t| c.text_of(t) == "static")
        else {
            continue;
        };
        let name = c.tree.child(n, Name).map(|x| c.trimmed(x)).unwrap_or_else(|| "unnamed".into());
        s.add(
            "SV-RND-009",
            "StaticConstraint",
            Severity::Warning,
            format!(
                "Constraint '{name}' is declared 'static'. In SystemVerilog (IEEE 1800 § 18.5.2), static constraints are shared across all instances of the class; \
                 dynamically disabling or altering them creates cross-instance state pollution and non-deterministic test failures."
            ),
            c.loc(st),
        );
    }
}
