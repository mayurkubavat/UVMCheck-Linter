//! SV-RDB-005 (RedundantBoolean): Flags 1-bit values compared with 1'b1/1'b0
//! and 'if (c) return 1; else return 0;' (Sonar S1125, clang-tidy
//! readability-simplify-boolean-expr).

use crate::expr_types::expr_types;
use crate::support::config_db_calls::{bool_literal, in_generate_header, strip, user_visits, with_parens};
use crate::support::slang_ast::kids;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

/// A statement under `begin ... end` blocks of one statement.
fn unwrap_stmt(c: &Cst, mut s: NodeId) -> NodeId {
    while c.kind(s) == SeqBlock {
        let items = c.items(s);
        let stmts: Vec<NodeId> =
            items.iter().copied().filter(|&x| crate::support::statements::stmt(c, x).is_some()).collect();
        match stmts.as_slice() {
            [one] if crate::support::statements::stmt(c, *one) != Some(crate::support::statements::St::Decl) => {
                s = *one
            }
            _ => break,
        }
    }
    s
}

fn returned_bool(c: &Cst, s: NodeId) -> Option<i128> {
    let x = unwrap_stmt(c, s);
    if c.kind(x) != ReturnStmt {
        return None;
    }
    let e = kids(c, x).into_iter().find(|&k| crate::expr_types::is_expr(c.kind(k)))?;
    bool_literal(c, e)
}

/// `b == 1'b1` on a 1-bit `b`, and `if (c) return 1; else return 0;`.
pub(crate) fn redundant_boolean(c: &Cst, out: &mut Sink) {
    let types = expr_types(c);
    let ix = c.index();
    let text = crate::support::type_text::expr_text;
    let mut found: Vec<(NodeId, String, Option<crate::fix::Fix>)> = Vec::new();
    for e in c.nodes(BinaryExpr) {
        let op = c.keyword(e).map_or("", |k| c.text_of(k));
        if !matches!(op, "==" | "!=") {
            continue;
        }
        let ks = kids(c, e);
        let (Some(&l), Some(&r)) = (ks.first(), ks.get(1)) else { continue };
        let (lb, rb) = (bool_literal(c, l), bool_literal(c, r));
        let (other, lit) = match (lb, rb) {
            (Some(v), _) => (r, v),
            (None, Some(v)) => (l, v),
            _ => continue,
        };
        if bool_literal(c, other).is_some() {
            continue;
        }
        let o = strip(c, other);
        let Some((w, _, four)) = types.get(&(ix, o)).and_then(|t| t.integral) else { continue };
        if w != 1 {
            continue;
        }
        // One bit only for some parameter values (`logic [W-1:0] cnt`).
        if crate::support::param_dep::width_depends_on_params(c, o) {
            continue;
        }
        if in_generate_header(c, e) {
            continue;
        }
        let bang = if (lit == 1) == (op == "==") { "" } else { "!" };
        found.push((
            e,
            format!(
                "'{}' compares a 1-bit value with a constant; write '{bang}{}' instead.",
                text(c, with_parens(c, e)),
                text(c, with_parens(c, o))
            ),
            simplify_fix(c, e, other, bang, four),
        ));
    }
    for n in c.nodes(IfStmt) {
        let (cond, then, els) = crate::support::statements::if_parts(c, n);
        let (Some(cond), Some(then), Some(els)) = (cond, then, els) else { continue };
        let chained = c.parent(n).filter(|&p| c.kind(p) == ElseClause).is_some();
        if chained || c.kind(cond) == MatchExpr || c.toks(cond).any(|t| c.text_of(t) == "&&&") {
            continue;
        }
        let (Some(t), Some(f)) = (returned_bool(c, then), returned_bool(c, els)) else { continue };
        if t == f {
            continue;
        }
        found.push((
            n,
            format!(
                "'if (c) return {t}; else return {f};' can be written 'return {}({});'.",
                if t == 1 { "" } else { "!" },
                text(c, cond)
            ),
            // `return c;` truncates a wider `c` to the return type: no fix.
            None,
        ));
    }
    for (n, msg, fix) in found {
        let times = user_visits(c, n);
        if times == 0 || crate::support::statements::in_bad_body(c, n) {
            continue;
        }
        let Some(t) = c.first_tok(n) else { continue };
        for _ in 0..times {
            let f = out.add("SV-RDB-005", "RedundantBoolean", Severity::Info, msg.clone(), c.loc(t));
            f.fixes.extend(fix.clone());
        }
    }
}

/// `b == 1'b1` -> `b`, `b == 1'b0` -> `!b` (and the `!=` forms). `!b` and
/// `b == 1'b0` agree for every value of a 1-bit `b`, X and Z included (both
/// give X). `b` is Z where `b == 1'b1` is X, which only an `if`, loop or `?:`
/// condition treats the same (false), so outside a condition that form is
/// safe only on a 2-state `b`.
fn simplify_fix(c: &Cst, e: NodeId, other: NodeId, bang: &str, four_state: bool) -> Option<crate::fix::Fix> {
    let outer = with_parens(c, e);
    let operand = c.trimmed(other);
    let primary =
        matches!(c.kind(other), NameExpr | MemberExpr | SelectExpr | CallExpr | ParenExpr | ScopedExpr | Literal);
    let text = match (bang, primary) {
        ("", _) => operand,
        (_, true) => format!("!{operand}"),
        (_, false) => format!("!({operand})"),
    };
    let condition = c.parent(outer).is_some_and(|p| match c.kind(p) {
        IfStmt => crate::support::statements::if_parts(c, p).0 == Some(outer),
        LoopStmt => matches!(
            crate::support::statements::stmt(c, p),
            Some(crate::support::statements::St::While | crate::support::statements::St::DoWhile)
        ),
        CondExpr => kids(c, p).first() == Some(&outer),
        _ => false,
    });
    let (a, b) = (c.first_tok(e)?, c.tree.token_range(e).last().copied()?);
    let fix = c.token_fix(a, b, text.clone(), &format!("Write '{text}'"))?;
    let safe = !bang.is_empty() || condition || !four_state;
    let safety = if safe { crate::fix::FixSafety::Safe } else { crate::fix::FixSafety::Unsafe };
    crate::fix::with_safety(Some(fix), safety)
}
