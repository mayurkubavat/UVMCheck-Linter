//! Name tokens, cross parts and port directions.

use sv_syntax::{Child, NodeId};

use crate::syntax::Cst;

use sv_syntax::SyntaxKind::*;

/// The name token of a declaration's `Name` child.
pub(crate) fn name_tok(c: &Cst, n: NodeId) -> Option<u32> {
    let name = c.tree.child(n, Name)?;
    c.tree.token_bounds(name).map(|(_, b)| b)
}

/// A cross's label and the coverpoints it names (slang's `items`).
pub(crate) fn cross_parts(c: &Cst, n: NodeId) -> (Option<String>, Vec<NodeId>) {
    let label = c.tree.child(n, StmtLabel).and_then(|l| c.keyword(l)).map(|t| c.text_of(t).to_string());
    let mut items = Vec::new();
    for k in c.kids(n) {
        match *k {
            Child::Token(t) if matches!(c.text_of(t), "iff" | "{" | ";") => break,
            Child::Node(x) if c.kind(x) == NameExpr => items.push(x),
            _ => {}
        }
    }
    (label, items)
}

pub(crate) const DIRECTIONS: [&str; 4] = ["input", "output", "inout", "ref"];
