//! Node spans and children.

use sv_syntax::NodeId;

use crate::syntax::Cst;

/// slang's `toString()` of the tokens `a..=b`.
pub(crate) fn span(c: &Cst, a: u32, b: u32) -> String {
    let mut s = String::new();
    for t in a..=b {
        let tok = &c.tree.tokens[t as usize];
        if !tok.from_macro {
            s.push_str(c.tree.trivia(t));
        }
        s.push_str(&tok.text);
    }
    s
}

pub(crate) fn child_nodes(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.tree.children(n).collect()
}
