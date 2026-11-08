//! Operators, keywords and for-loop parts.

use sv_syntax::{Child, NodeId};

use crate::syntax::Cst;

use sv_syntax::SyntaxKind::*;

pub(crate) fn op_text<'t>(c: &Cst<'t>, n: NodeId) -> &'t str {
    c.toks(n).next().map_or("", |t| c.text_of(t))
}

pub(crate) fn kids(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.tree.children(n).collect()
}

pub(crate) fn keyword_is(c: &Cst, n: NodeId, kw: &str) -> bool {
    c.keyword(n).is_some_and(|k| c.text_of(k) == kw)
}

/// A `for` loop's initializers, stop expression and body.
pub(crate) fn for_parts(c: &Cst, l: NodeId) -> (Vec<NodeId>, Option<NodeId>, Option<NodeId>) {
    let mut semis = 0;
    let (mut init, mut stop) = (Vec::new(), None);
    for k in c.kids(l) {
        match *k {
            Child::Token(t) if c.text_of(t) == ";" => semis += 1,
            Child::Node(n) if semis == 0 => init.push(n),
            Child::Node(n) if semis == 1 => stop = Some(n),
            _ => {}
        }
    }
    (init, stop, kids(c, l).last().copied())
}

/// Every node under `n` (itself included), in source order.
pub(crate) fn subtree(c: &Cst, n: NodeId, out: &mut Vec<NodeId>) {
    match c.tree.subtree_slice(n) {
        Some(s) => out.extend_from_slice(s),
        None => out.extend(c.tree.subtree(n)),
    }
}

/// The predicates slang keeps as a `ConditionalPredicate`: an `if`
/// condition and a `?:` condition.
pub(crate) fn predicates(c: &Cst) -> Vec<NodeId> {
    let mut out: Vec<NodeId> = c.nodes(IfStmt).into_iter().filter_map(|n| kids(c, n).first().copied()).collect();
    out.extend(c.nodes(CondExpr).into_iter().filter_map(|n| kids(c, n).first().copied()));
    out
}

/// Names a `for` loop declares or assigns in its initializers.
pub(crate) fn for_vars(c: &Cst, init: &[NodeId], first_token: bool) -> Vec<String> {
    let mut vars = Vec::new();
    for &i in init {
        match c.kind(i) {
            DataDecl => {
                for d in kids(c, i).into_iter().filter(|&d| c.kind(d) == Declarator) {
                    if let Some(nm) = c.tree.child(d, Name) {
                        vars.push(c.tree.bare_text(nm));
                    }
                }
            }
            BinaryExpr if op_text(c, i) == "=" => {
                let Some(&l) = kids(c, i).first() else { continue };
                if first_token {
                    if let Some(t) = c.first_tok(l) {
                        vars.push(c.text_of(t).to_string());
                    }
                } else if is_ident_name(c, l) {
                    vars.push(c.tree.bare_text(l));
                }
            }
            _ => {}
        }
    }
    vars
}

/// slang's `IdentifierName`: a plain name.
pub(crate) fn is_ident_name(c: &Cst, n: NodeId) -> bool {
    c.kind(n) == NameExpr
        && c.tree.token_bounds(n).is_some_and(|(a, b)| {
            a == b && {
                let t = c.text_of(a);
                t.starts_with(|ch: char| ch.is_ascii_alphabetic() || ch == '_')
                    && !matches!(t, "this" | "super" | "null" | "local")
            }
        })
}
