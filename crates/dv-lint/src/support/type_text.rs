//! Expression text and type names.

use std::collections::HashMap;

use sv_syntax::NodeId;




use crate::syntax::Cst;



use sv_syntax::SyntaxKind::*;

/// Tokens joined, any trivia between two of them read as one space
/// (swe::exprText).
pub(crate) fn expr_text(c: &Cst, n: NodeId) -> String {
    let mut out = String::new();
    for (i, t) in c.tree.token_range(n).into_iter().enumerate() {
        let tok = &c.tree.tokens[t as usize];
        if i > 0 && (tok.space_before || !c.tree.trivia(t).is_empty()) {
            out.push(' ');
        }
        out.push_str(&tok.text);
    }
    out
}

/// The name of the class a class extends, without parameters.
pub(crate) fn base_name(c: &Cst, cls: NodeId) -> Option<String> {
    let ext = c.tree.child(cls, Header).and_then(|h| c.tree.child(h, Extends))?;
    let base = c.tree.children(ext).next()?;
    Some(
        c.tree
            .children(base)
            .find(|&k| c.kind(k) == ParamList)
            .and_then(|p| c.first_tok(p))
            .map_or_else(|| c.last_text(base).to_string(), |p| c.text_of(p - 1).to_string()),
    )
}

/// The class name a data type names (`pkg::C #(T)` gives `C`).
pub(crate) fn type_name(c: &Cst, dt: NodeId) -> Option<String> {
    let first = c.first_tok(dt)?;
    if c.tree.tokens[first as usize].kind != sv_syntax::TokenKind::Identifier {
        return None;
    }
    let end = c
        .tree
        .children(dt)
        .find(|&k| matches!(c.kind(k), ParamList | Dim))
        .and_then(|p| c.first_tok(p))
        .map_or_else(|| c.tree.token_bounds(dt).map(|b| b.1), |p| Some(p - 1))?;
    Some(c.text_of(end).to_string())
}

pub(crate) fn is_class_type(c: &Cst, dt: Option<NodeId>, classes: &HashMap<String, NodeId>) -> bool {
    dt.and_then(|d| type_name(c, d))
        .is_some_and(|n| classes.contains_key(&n) || matches!(n.as_str(), "mailbox" | "semaphore" | "process"))
}
