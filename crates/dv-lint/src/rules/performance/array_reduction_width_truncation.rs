//! SV-PRF-002 (ArrayReductionWidthTruncation): Array reduction methods (.sum,
//! .product) accumulate at element bit-width before extension to LHS.

use sv_syntax::SyntaxKind::*;
use crate::support::slang_ast::kids;
use crate::expr_types::expr_types;
use crate::support::style_config::ast_visits;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::config_db_calls::{strip};

/// `x = a.sum()` (or `.product()`) into a target wider than the elements:
/// once per AST visit.
pub(crate) fn array_reduction_width(c: &Cst, out: &mut Sink) {
    let types = expr_types(c);
    let ix = c.index();
    for e in c.nodes(BinaryExpr) {
        let op = c.keyword(e).map_or("", |k| c.text_of(k));
        let assign = matches!(op, "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=" | "<<<=" | ">>>=")
            || (op == "<=" && c.parent(e).is_some_and(|p| c.kind(p) == ExprStmt));
        if !assign {
            continue;
        }
        let ks = kids(c, e);
        let (Some(&l), Some(&r)) = (ks.first(), ks.get(1)) else { continue };
        let call = strip(c, r);
        if c.kind(call) != CallExpr {
            continue;
        }
        let Some(&callee) = kids(c, call).first() else { continue };
        let name = c.last_text(callee);
        if c.kind(callee) != MemberExpr || !matches!(name, "sum" | "product") {
            continue;
        }
        let lw = types.get(&(ix, l)).and_then(|t| t.integral).map_or(0, |x| x.0);
        let Some(ew) = types.get(&(ix, call)).and_then(|t| t.integral).map(|x| x.0) else { continue };
        // Any `with` in the call's text.
        if lw <= ew || c.tree.bare_text(call).contains("with") {
            continue;
        }
        let Some(t) = c.first_tok(call) else { continue };
        for _ in 0..ast_visits(c, e) {
            out.add(
                "SV-PRF-002",
                "ArrayReductionWidthTruncation",
                Severity::Error,
                format!(
                    "Array reduction method '.{name}()' accumulates using the array element width ({ew} bits) before assignment to wider target ({lw} bits). Sum/product will silently wrap and truncate. Use with clause to expand accumulator, e.g. '.sum(item) with (int'(item))'."
                ),
                c.loc(t),
            );
        }
    }
}
