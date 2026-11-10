//! SV-RND-008 (NonInvertibleConstraintOp): Disallow non-invertible operators
//! (%, /, **) inside constraint blocks.

use crate::support::class_items::{class_items, decl_name, model_classes, op, subtree};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind::*;

pub(crate) fn non_invertible_op(c: &Cst, s: &mut Sink) {
    for (cls, _) in model_classes(c) {
        for it in class_items(c, cls) {
            if c.kind(it) != ConstraintDecl {
                continue;
            }
            let Some((block, _)) = decl_name(c, it) else { continue };
            for n in subtree(c, it) {
                if c.kind(n) != BinaryExpr {
                    continue;
                }
                let what = match op(c, n) {
                    "%" => "% (modulo)",
                    "/" => "/ (division)",
                    "**" => "** (exponentiation)",
                    _ => continue,
                };
                let Some(at) = c.first_tok(n) else { continue };
                s.add(
                    "SV-RND-008",
                    "NonInvertibleConstraintOp",
                    Severity::Warning,
                    format!(
                        "Non-invertible mathematical operator '{what}' used inside constraint block '{block}'. Non-linear arithmetic cannot be mapped efficiently to 2-state SAT/SMT solver equations, triggering exponential backtracking and simulator stalls."
                    ),
                    c.loc(at),
                );
            }
        }
    }
}
