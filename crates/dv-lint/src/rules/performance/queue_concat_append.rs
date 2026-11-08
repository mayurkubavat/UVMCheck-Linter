//! SV-PRF-007 (QueueConcatAppend): Disallow queue concatenation ('q = {q, x}'
//! or 'q = {x, q}').

use crate::support::style_config::ast_visits;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

/// The declarator has a queue dimension (`[$]`, `[$:N]`), or its type is a
/// typedef of a queue.
fn is_queue(c: &Cst, d: &crate::support::slang_ast::Decl) -> bool {
    let q = |t: &sv_syntax::Tree, n: NodeId| {
        t.children(n).filter(|&k| t.kind(k) == Dim).any(|k| {
            let text = t.bare_text(k);
            text.trim_start_matches('[').trim_start().starts_with('$')
        })
    };
    if c.kind(d.node) == Declarator && q(c.tree, d.node) {
        return true;
    }
    // The typedef's own name (the last of `pkg::cls::name_t`).
    let Some(name) = d
        .dtype
        .and_then(|ty| {
            c.toks(ty).take_while(|&t| !matches!(c.text_of(t), "[" | "#")).filter(|&t| c.text_of(t) != "::").last()
        })
        .map(|t| c.text_of(t).to_string())
    else {
        return false;
    };
    c.all.iter().any(|t| {
        (0..t.nodes.len())
            .map(|i| NodeId(i as u32))
            .any(|n| t.kind(n) == TypedefDecl && t.child(n, Name).is_some_and(|nm| t.bare_text(nm) == name) && q(t, n))
    })
}

/// `q = {q, x}` or `q = {x, q}` on a queue: once per AST visit.
pub(crate) fn queue_concat_append(c: &Cst, out: &mut Sink) {
    for e in c.nodes(BinaryExpr) {
        let op = c.keyword(e).map_or("", |k| c.text_of(k));
        let assign = op == "=" || (op == "<=" && c.parent(e).is_some_and(|p| c.kind(p) == ExprStmt));
        if !assign {
            continue;
        }
        let ks = crate::support::slang_ast::kids(c, e);
        let (Some(&lhs), Some(&rhs)) = (ks.first(), ks.get(1)) else { continue };
        if c.kind(rhs) != ConcatExpr {
            continue;
        }
        let ops = crate::support::slang_ast::kids(c, rhs);
        if ops.len() != 2 {
            continue;
        }
        let Some(ld) = crate::support::handles::expr_decl(c, lhs) else { continue };
        if !is_queue(c, &ld) {
            continue;
        }
        // slang's `getSymbolReference`: through element and range selects.
        let same = |x: NodeId| {
            let mut x = x;
            while c.kind(x) == SelectExpr {
                match crate::support::slang_ast::kids(c, x).first() {
                    Some(&b) => x = b,
                    None => return false,
                }
            }
            crate::support::handles::expr_decl(c, x).is_some_and(|d| d.node == ld.node)
        };
        let method = if same(ops[0]) {
            "push_back"
        } else if same(ops[1]) {
            "push_front"
        } else {
            continue;
        };
        let name = if c.kind(lhs) == MemberExpr { c.last_text(lhs).to_string() } else { c.tree.bare_text(lhs) };
        let Some(t) = c.first_tok(lhs) else { continue };
        let item = if method == "push_back" { ops[1] } else { ops[0] };
        let item_text = c.trimmed(item);
        let fix = (!item_text.is_empty())
            .then(|| {
                let (a, b) = c.tree.token_bounds(e)?;
                let f = c.token_fix(
                    a,
                    b,
                    format!("{name}.{method}({item_text})"),
                    &format!("Replace concatenation with {method}()"),
                );
                // 'q <= {q, x}' updates q in the NBA region; push_back() updates it now.
                if op == "<=" {
                    crate::fix::with_safety(f, crate::fix::FixSafety::Unsafe)
                } else {
                    f
                }
            })
            .flatten();
        for _ in 0..ast_visits(c, e) {
            out.add(
                "SV-PRF-007",
                "QueueConcatAppend",
                Severity::Warning,
                format!(
                    "Queue concatenation '{name} = {{ ... }}' causes O(N) memory reallocation and buffer copying on every operation. \
                     In loops or transaction streams, this degrades into an O(N^2) memory churn bottleneck. \
                     Use in-place '{name}.{method}(...)' instead for amortized O(1) performance."
                ),
                c.loc(t),
            )
            .fixes
            .extend(fix.clone());
        }
    }
}
