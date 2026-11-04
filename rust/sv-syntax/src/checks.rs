//! Checks Slang's parser makes beyond the grammar, run over the finished
//! tree: end labels that do not match, tasks with a return type, crosses
//! of a single item, and `super.new` after other statements.

use crate::tree::{Child, SyntaxError};
use crate::{NodeId, SyntaxKind, Tree};
use SyntaxKind::*;

pub(crate) fn run(tree: &Tree) -> Vec<SyntaxError> {
    let mut out = Vec::new();
    for n in tree.preorder() {
        match tree.kind(n) {
            TaskDecl | MethodPrototype => task_return_type(tree, n, &mut out),
            Cross => cross_items(tree, n, &mut out),
            _ => {}
        }
        if tree.kind(n) == FunctionDecl {
            super_new(tree, n, &mut out);
        }
        end_label(tree, n, &mut out);
    }
    out.sort_by_key(|e| e.token);
    out
}

fn text(tree: &Tree, t: u32) -> &str {
    &tree.tokens[t as usize].text
}

fn last_token(tree: &Tree, n: NodeId) -> Option<u32> {
    tree.token_range(n).last().copied()
}

fn first_tokens(tree: &Tree, n: NodeId, k: usize) -> Vec<&str> {
    tree.token_range(n).into_iter().take(k).map(|t| text(tree, t)).collect()
}

fn child_nodes(tree: &Tree, n: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    tree.node(n).children.iter().filter_map(|c| match *c {
        Child::Node(x) => Some(x),
        Child::Token(_) => None,
    })
}

/// `endmodule : other` names something else.
fn end_label(tree: &Tree, n: NodeId, out: &mut Vec<SyntaxError>) {
    let labels: Vec<NodeId> = child_nodes(tree, n).filter(|&c| tree.kind(c) == EndLabel).collect();
    let Some(&end) = labels.last() else { return };
    let Some(label) = last_token(tree, end) else { return };
    let name = match child_nodes(tree, n).find(|&c| tree.kind(c) == Name) {
        // A declaration: the label repeats its name (the method name of
        // `C::f`).
        Some(name) => last_token(tree, name),
        // A block: `begin : a ... end : a`.
        None if labels.len() == 2 => last_token(tree, labels[0]),
        None => None,
    };
    if let Some(name) = name {
        if text(tree, name) != text(tree, label) && text(tree, label) != ":" {
            out.push(SyntaxError {
                token: label,
                message: format!("end label `{}` does not match `{}`", text(tree, label), text(tree, name)),
            });
        }
    }
}

fn task_return_type(tree: &Tree, n: NodeId, out: &mut Vec<SyntaxError>) {
    let is_task = tree.node(n).children.iter().any(|c| matches!(*c, Child::Token(t) if text(tree, t) == "task"));
    if !is_task {
        return;
    }
    if let Some(r) = child_nodes(tree, n).find(|&c| tree.kind(c) == ReturnType) {
        if let Some(&t) = tree.token_range(r).first() {
            out.push(SyntaxError { token: t, message: "a task cannot have a return type".into() });
        }
    }
}

/// `cross a;`: a cross needs at least two items.
fn cross_items(tree: &Tree, n: NodeId, out: &mut Vec<SyntaxError>) {
    let mut seen_cross = false;
    let mut items = 0;
    let mut at = None;
    for c in &tree.node(n).children {
        match *c {
            Child::Token(t) => {
                let s = text(tree, t);
                if s == "cross" {
                    seen_cross = true;
                    at = Some(t);
                } else if seen_cross && matches!(s, "iff" | "{" | ";") {
                    break;
                }
            }
            Child::Node(x) if seen_cross && tree.kind(x) != Attribute => items += 1,
            Child::Node(_) => {}
        }
    }
    if let (Some(t), true) = (at, items < 2) {
        out.push(SyntaxError { token: t, message: "a cross needs at least two items".into() });
    }
}

/// In a constructor, `super.new(...)` must come before other statements.
fn super_new(tree: &Tree, n: NodeId, out: &mut Vec<SyntaxError>) {
    let Some(name) = child_nodes(tree, n).find(|&c| tree.kind(c) == Name) else { return };
    if last_token(tree, name).map(|t| text(tree, t)) != Some("new") {
        return;
    }
    let Some(body) = child_nodes(tree, n).find(|&c| tree.kind(c) == Body) else { return };
    let mut statements = 0;
    for s in child_nodes(tree, body) {
        match tree.kind(s) {
            DataDecl | ParamDecl | TypedefDecl | ImportDecl | LetDecl | ForwardTypedef => {}
            ExprStmt if first_tokens(tree, s, 3) == ["super", ".", "new"] => {
                if statements > 0 {
                    let t = tree.token_range(s)[0];
                    out.push(SyntaxError {
                        token: t,
                        message: "`super.new` must be the first statement of a constructor".into(),
                    });
                }
                return;
            }
            _ => statements += 1,
        }
    }
}
