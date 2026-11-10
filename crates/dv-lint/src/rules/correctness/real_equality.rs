//! SV-BUG-006 (RealEquality): Flags == and != on real or shortreal operands,
//! except against an exact zero (Sonar S1244, clang-tidy float equality).

use crate::support::class_items::{kids, op, user_code};
use crate::support::data_items::unparen;
use crate::support::real_types::{Ty, Types};
use crate::support::sema_facts::ast_ok;
use crate::support::type_text::expr_text;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

fn is_zero_literal(c: &Cst, e: NodeId) -> bool {
    let e = unparen(c, e);
    if c.kind(e) != Literal {
        return false;
    }
    let t = c.tree.bare_text(e).replace('_', "");
    let digits = match t.find('\'') {
        Some(q) => t[q + 1..].trim_start_matches(['s', 'S']).get(1..).unwrap_or("").to_string(),
        None => t,
    };
    !digits.is_empty() && digits.parse::<f64>().is_ok_and(|v| v == 0.0)
}

pub(crate) fn real_equality(c: &Cst, s: &mut Sink) {
    let types = Types::new(c);
    for n in c.nodes(BinaryExpr) {
        if !matches!(op(c, n), "==" | "!=" | "===" | "!==") || !user_code(c, n) {
            continue;
        }
        let k = kids(c, n);
        let (Some(&l), Some(&r)) = (k.first(), k.get(1)) else { continue };
        if types.expr(l) != Some(Ty::Real) && types.expr(r) != Some(Ty::Real) {
            continue;
        }
        if is_zero_literal(c, l) || is_zero_literal(c, r) || !ast_ok(c, n) {
            continue;
        }
        let Some(at) = c.first_tok(n) else { continue };
        s.add(
            "SV-BUG-006",
            "RealEquality",
            Severity::Warning,
            format!(
                "'{}' compares real values for exact equality. Rounding makes this fail for values that are equal in \
                 practice; compare the difference against a tolerance instead.",
                expr_text(c, n)
            ),
            c.loc(at),
        );
    }
}
