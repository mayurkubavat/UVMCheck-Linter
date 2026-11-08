//! A canonical dump of the operator expressions in a tree, for checking the
//! parser against Slang (tools/cst_exprs.cpp prints the same form): every
//! top-level binary, unary, conditional and inside expression, written in
//! prefix form, `(+ a (* b c))`, everything else as its tokens without
//! trivia. Regions kept as written (headers, constraints, assertions,
//! timing controls, ...) are skipped.

use crate::tree::Child;
use crate::{NodeId, SyntaxKind, Tree};
use SyntaxKind::*;

fn is_op(k: SyntaxKind) -> bool {
    matches!(k, BinaryExpr | PrefixExpr | PostfixExpr | CondExpr | InsideExpr)
}

fn skipped(k: SyntaxKind) -> bool {
    // Nothing is skipped any more; kept so the gate can exclude a kind
    // again while a construct is being brought up.
    matches!(k, OtherBlock)
}

/// Every top-level operator expression of `tree`, in source order, in the
/// canonical form described above.
pub fn canonical_exprs(tree: &Tree) -> Vec<String> {
    let mut out = Vec::new();
    walk(tree, tree.root, false, &mut out);
    out
}

fn walk(tree: &Tree, n: NodeId, under: bool, out: &mut Vec<String>) {
    let k = tree.kind(n);
    if skipped(k) {
        return;
    }
    let mut under = under;
    if is_op(k) && !under {
        let mut s = String::new();
        canon(tree, n, &mut s);
        out.push(s);
        under = true;
    }
    for c in tree.children(n) {
        walk(tree, c, under, out);
    }
}

fn parts(tree: &Tree, n: NodeId) -> (Vec<NodeId>, Vec<u32>) {
    let mut nodes = Vec::new();
    let mut toks = Vec::new();
    for c in &tree.node(n).children {
        match *c {
            // Like Slang's, the canonical form leaves attributes out.
            Child::Node(x) if tree.kind(x) == Attribute => {}
            Child::Node(x) => nodes.push(x),
            Child::Token(t) => toks.push(t),
        }
    }
    (nodes, toks)
}

fn canon(tree: &Tree, n: NodeId, s: &mut String) {
    let text = |t: u32| -> &str { &tree.tokens[t as usize].text };
    let (nodes, toks) = parts(tree, n);
    if skipped(tree.kind(n)) {
        for t in tree.token_range(n) {
            s.push_str(text(t));
        }
        return;
    }
    match tree.kind(n) {
        BinaryExpr if nodes.len() == 2 && !toks.is_empty() => {
            s.push('(');
            s.push_str(text(toks[0]));
            s.push(' ');
            canon(tree, nodes[0], s);
            s.push(' ');
            canon(tree, nodes[1], s);
            s.push(')');
        }
        PrefixExpr if nodes.len() == 1 && !toks.is_empty() => {
            s.push('(');
            s.push_str(text(toks[0]));
            s.push(' ');
            canon(tree, nodes[0], s);
            s.push(')');
        }
        PostfixExpr if nodes.len() == 1 && !toks.is_empty() => {
            s.push_str("(post");
            s.push_str(text(toks[0]));
            s.push(' ');
            canon(tree, nodes[0], s);
            s.push(')');
        }
        CondExpr if nodes.len() == 3 => {
            s.push_str("(? ");
            canon(tree, nodes[0], s);
            s.push(' ');
            canon(tree, nodes[1], s);
            s.push(' ');
            canon(tree, nodes[2], s);
            s.push(')');
        }
        InsideExpr if nodes.len() == 2 => {
            s.push_str("(inside ");
            canon(tree, nodes[0], s);
            s.push(' ');
            canon(tree, nodes[1], s);
            s.push(')');
        }
        _ => {
            for c in &tree.node(n).children {
                match *c {
                    Child::Node(x) if tree.kind(x) == Attribute => {}
                    Child::Node(x) => canon(tree, x, s),
                    Child::Token(t) => s.push_str(text(t)),
                }
            }
        }
    }
}
