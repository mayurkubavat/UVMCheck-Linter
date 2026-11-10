//! SV-RND-022 (ConstraintCallingNonPureFunction): Functions called in
//! constraints must be pure and must not have side effects or void return
//! types.

use crate::support::class_items::{kids, with_clause};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

fn non_pure_walk(c: &Cst, n: NodeId, inside: &mut bool, s: &mut Sink) {
    let k = c.kind(n);
    if matches!(k, ConstraintDecl | ConstraintBlock) {
        *inside = true;
        for x in kids(c, n) {
            non_pure_walk(c, x, inside, s);
        }
        *inside = false;
        return;
    }
    if k == CallExpr && *inside && with_clause(c, n).is_none() {
        if let Some(left) = c.tree.children(n).next() {
            let f = c.last_text(left);
            let non_pure = ["print", "display", "update", "set_", "void", "task"].iter().any(|x| f.contains(x));
            if !matches!(f, "inside" | "dist" | "size") && non_pure {
                if let Some(at) = c.first_tok(left) {
                    s.add(
                        "SV-RND-022",
                        "ConstraintCallingNonPureFunction",
                        Severity::Error,
                        format!(
                            "Function '{f}' invoked inside constraint block appears non-pure or mutates state. \
                             In SystemVerilog (IEEE 1800 § 18.5.12), functions called inside constraint \
                             expressions must be pure: they cannot modify class state, call tasks, or have void return types. \
                             Impure functions cause constraint solver instability and unpredictable solution distributions."
                        ),
                        c.loc(at),
                    );
                }
            }
        }
    }
    for x in kids(c, n) {
        non_pure_walk(c, x, inside, s);
    }
}

pub(crate) fn non_pure_in_constraint(c: &Cst, s: &mut Sink) {
    let mut inside = false;
    non_pure_walk(c, c.tree.root, &mut inside, s);
}
