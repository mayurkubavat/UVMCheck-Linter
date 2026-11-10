//! SV-SVA-002 (DisableIffComplexExpression): Flags 'disable iff (...)' clauses
//! containing complex boolean expressions or multi-variable logic instead of
//! simple reset conditions.

use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

fn is_ident_name(c: &Cst, e: NodeId) -> bool {
    if c.kind(e) != NameExpr || c.tree.children(e).next().is_some() {
        return false;
    }
    let toks: Vec<u32> = c.toks(e).collect();
    toks.len() == 1 && {
        let t = c.text_of(toks[0]);
        t.starts_with(|ch: char| ch.is_ascii_alphabetic() || ch == '_' || ch == '\\')
            && !matches!(t, "this" | "super" | "null")
    }
}

/// slang's integer literal or vector expression (`5`, `1'b0`, `'h3`), not
/// an unbased unsized `'0`.
fn is_int_literal(c: &Cst, e: NodeId) -> bool {
    if c.kind(e) != Literal {
        return false;
    }
    let text: String = c.tree.token_range(e).into_iter().map(|t| c.text_of(t)).collect();
    if text.bytes().all(|b| b.is_ascii_digit() || b == b'_') && !text.is_empty() {
        return true;
    }
    match text.find('\'') {
        Some(q) => {
            let rest = text[q + 1..].trim_start_matches(['s', 'S']);
            rest.starts_with(['d', 'D', 'h', 'H', 'b', 'B', 'o', 'O'])
        }
        None => false,
    }
}

fn is_simple_reset(c: &Cst, e: NodeId) -> bool {
    match c.kind(e) {
        NameExpr => is_ident_name(c, e),
        PrefixExpr => {
            let op = c.keyword(e).map(|t| c.text_of(t)).unwrap_or("");
            matches!(op, "!" | "~") && kids(c, e).first().is_some_and(|&x| is_ident_name(c, x))
        }
        ParenExpr => kids(c, e).first().is_some_and(|&x| is_simple_reset(c, x)),
        BinaryExpr => {
            let op = c.keyword(e).map(|t| c.text_of(t)).unwrap_or("");
            let k = kids(c, e);
            matches!(op, "==" | "!=") && k.len() == 2 && is_ident_name(c, k[0]) && is_int_literal(c, k[1])
        }
        _ => false,
    }
}

pub(crate) fn disable_iff_complex(c: &Cst, s: &mut Sink) {
    for d in c.nodes(DisableIff) {
        let Some(pe) = c.parent(d) else { continue };
        if c.kind(pe) != PropertyExpr {
            continue;
        }
        if !c.parent(pe).is_some_and(|p| matches!(c.kind(p), PropertyDecl | ConcurrentAssert)) {
            continue;
        }
        let Some(&e) = kids(c, d).first() else { continue };
        if is_simple_reset(c, e) {
            continue;
        }
        let Some(kw) = c.tok(d, "disable") else { continue };
        let text = slang_text(c, e);
        s.add(
            "SV-SVA-002",
            "DisableIffComplexExpression",
            Severity::Warning,
            format!(
                "'disable iff (...)' clause contains complex expression '{text}'. \
                 In SystemVerilog assertion guidelines (IEEE 1800 § 16.12), \
                 'disable iff' evaluates asynchronously on every signal glitch and should be strictly restricted to \
                 simple reset signals (e.g. '!rst_n') to avoid glitch sensitivity and formal property state explosion."
            ),
            c.loc(kw),
        );
    }
}
