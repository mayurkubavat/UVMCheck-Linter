//! Labels, last lines and cross parts.

use svlang::syntax::{Child, NodeId};

use crate::syntax::Cst;

use svlang::syntax::SyntaxKind::*;

pub(crate) fn kids(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.tree.children(n).collect()
}

/// slang's `toString()` reduced to its last line, trimmed.
pub(crate) fn last_line(text: &str) -> &str {
    text.rsplit('\n').next().unwrap_or("").trim_matches([' ', '\t', '\r'])
}

/// A coverpoint's or cross's label.
pub(crate) fn label(c: &Cst, n: NodeId) -> Option<String> {
    let l = c.tree.child(n, StmtLabel)?;
    c.toks(l).next().map(|t| c.text_of(t).to_string())
}

/// A cross: the crossed items, its `{ }` members and its keyword.
pub(crate) fn cross_parts(c: &Cst, n: NodeId) -> (usize, Vec<NodeId>, Option<u32>) {
    let mut items = 0;
    let mut members = Vec::new();
    let mut in_body = false;
    for k in c.kids(n) {
        match *k {
            Child::Token(t) if c.text_of(t) == "{" => in_body = true,
            Child::Node(x) if in_body => members.push(x),
            Child::Node(x) if !matches!(c.kind(x), StmtLabel | Attribute | EventExpr) => items += 1,
            _ => {}
        }
    }
    (items, members, c.tok(n, "cross"))
}
