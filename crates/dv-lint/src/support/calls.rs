//! Callees and arguments of statement calls.

use sv_syntax::{Child, NodeId};

use crate::syntax::Cst;

use sv_syntax::SyntaxKind::*;

pub(crate) fn kids(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.tree.children(n).collect()
}

/// A call's arguments as slang counts them: one per comma-separated slot,
/// `None` for an empty one; none at all for `()`.
pub(crate) fn args(c: &Cst, call: NodeId) -> Vec<Option<NodeId>> {
    let Some(list) = c.tree.child(call, ArgList) else { return Vec::new() };
    let mut out = vec![None];
    for k in c.kids(list) {
        match *k {
            Child::Token(t) if c.text_of(t) == "," => out.push(None),
            Child::Node(a) => *out.last_mut().unwrap() = Some(a),
            _ => {}
        }
    }
    if out.len() == 1 && out[0].is_none() {
        out.clear();
    }
    out
}

pub(crate) fn callee(c: &Cst, call: NodeId) -> Option<NodeId> {
    c.tree.children(call).next()
}

/// The call an expression statement makes, directly or under a cast
/// (`void'(...)` is slang's own statement kind, not a cast).
pub(crate) fn statement_calls(c: &Cst) -> Vec<NodeId> {
    let mut out = Vec::new();
    for st in c.nodes(ExprStmt) {
        let Some(e) = c.tree.children(st).next() else { continue };
        let call = match c.kind(e) {
            CallExpr => Some(e),
            CastExpr => {
                let k = kids(c, e);
                let signed =
                    k.first().is_some_and(|&t| matches!(c.tree.bare_text(t).as_str(), "signed" | "unsigned" | "void"));
                k.last().copied().filter(|&x| !signed && k.len() == 2 && c.kind(x) == CallExpr)
            }
            _ => None,
        };
        out.extend(call);
    }
    out
}
