//! Methods, data items and literals of classes, for the OOP rules.

use svlang::syntax::{Child, NodeId};

use crate::support::class_items::class_items;

use crate::syntax::Cst;

use svlang::syntax::SyntaxKind::*;

/// A class method: function or task declared in the class body.
pub(crate) fn methods(c: &Cst, cls: NodeId) -> Vec<NodeId> {
    class_items(c, cls).into_iter().filter(|&x| matches!(c.kind(x), FunctionDecl | TaskDecl)).collect()
}

pub(crate) fn routine_name(c: &Cst, f: NodeId) -> String {
    c.tree.child(f, Name).map(|n| c.last_text(n).to_string()).unwrap_or_default()
}

/// Data declarations among the class's items.
pub(crate) fn data_items(c: &Cst, cls: NodeId) -> Vec<NodeId> {
    class_items(c, cls).into_iter().filter(|&x| c.kind(x) == DataDecl && c.tree.child(x, DataType).is_some()).collect()
}

/// The files given to the compiler (not included ones): the tree's first.
pub(crate) fn main_files(c: &Cst) -> Vec<u32> {
    c.tree.mains.clone()
}

/// slang's IntegerLiteral or RealLiteral (not strings, times or `'0`).
pub(crate) fn is_number_literal(c: &Cst, n: NodeId) -> bool {
    if c.kind(n) != Literal {
        return false;
    }
    let t = c.trimmed(n);
    let t = t.as_str();
    let Some(f) = t.chars().next() else { return false };
    if f == '"' {
        return false;
    }
    if f == '\'' {
        // `'h1F` is an integer; `'0`, `'1`, `'x`, `'z` are unbased unsized.
        return t.len() > 2
            || t.chars()
                .nth(1)
                .is_some_and(|x| matches!(x, 'h' | 'H' | 'd' | 'D' | 'b' | 'B' | 'o' | 'O' | 's' | 'S'));
    }
    if !f.is_ascii_digit() {
        return false;
    }
    // A time literal ends in its unit.
    !["fs", "ps", "ns", "us", "ms", "s", "step"].iter().any(|u| t.ends_with(u) && !t.contains('\''))
}

/// Explicit casts are conversions in slang's AST; the operand keeps the
/// cast's parentheses in its syntax (`int'(j)` gives `(j)`). Gives the
/// operand and that text.
pub(crate) fn strip_conv(c: &Cst, e: NodeId) -> (NodeId, String) {
    let mut e = e;
    let mut text = c.text(e);
    while c.kind(e) == CastExpr {
        let (Some(open), Some(close)) = (c.tok(e, "("), c.toks(e).filter(|&t| c.text_of(t) == ")").last()) else {
            break;
        };
        let Some(x) = c.tree.children(e).nth(1) else { break };
        text = crate::support::class_items::span_text(c, open, close);
        e = x;
    }
    (e, text)
}

/// slang drops parentheses from the bound expression.
pub(crate) fn unparen(c: &Cst, mut e: NodeId) -> NodeId {
    while c.kind(e) == ParenExpr {
        match c.tree.children(e).next() {
            Some(x) => e = x,
            None => break,
        }
    }
    e
}

#[allow(dead_code)]
pub(crate) fn tokens_of(c: &Cst, n: NodeId) -> Vec<u32> {
    c.kids(n).iter().filter_map(|k| if let Child::Token(t) = *k { Some(t) } else { None }).collect()
}
