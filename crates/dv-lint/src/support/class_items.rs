//! Class declarations, their items, qualifiers and constraints, for the
//! randomization rules.

use sv_syntax::{Child, NodeId};

use crate::syntax::Cst;

use sv_syntax::SyntaxKind::*;

pub(crate) fn kids(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.tree.children(n).collect()
}

/// The first direct token's text ("" when none): a binary operator, a
/// statement keyword.
pub(crate) fn op<'t>(c: &Cst<'t>, n: NodeId) -> &'t str {
    c.toks(n).next().map(|t| c.text_of(t)).unwrap_or("")
}

/// Direct tokens' texts.
pub(crate) fn own<'t>(c: &Cst<'t>, n: NodeId) -> Vec<&'t str> {
    c.toks(n).map(|t| c.text_of(t)).collect()
}

/// `n` and every node below it, in pre-order.
pub(crate) fn subtree(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.tree.subtree(n)
}

/// Trims what C++'s `find_first_not_of(" \t\r\n")` trims.
pub(crate) fn trim(s: &str) -> &str {
    s.trim_matches([' ', '\t', '\r', '\n'])
}

/// A declaration's name: the last token of its Name child.
pub(crate) fn decl_name(c: &Cst, n: NodeId) -> Option<(String, u32)> {
    let name = c.tree.child(n, Name)?;
    let (_, last) = c.tree.token_bounds(name)?;
    Some((c.text_of(last).to_string(), last))
}

/// A class declaration with a parameter port list (a generic class, which
/// the UVM model does not index).
pub(crate) fn is_parameterized(c: &Cst, cls: NodeId) -> bool {
    c.tree
        .child(cls, Header)
        .and_then(|h| c.tree.child(h, ParamList))
        .is_some_and(|p| c.tree.children(p).next().is_some())
}

/// An out-of-block class declaration name such as `pc::inner` is not a
/// class of its own here.
pub(crate) fn class_decls(c: &Cst) -> Vec<NodeId> {
    c.nodes(ClassDecl)
}

/// The class's direct items (properties, methods, constraints).
pub(crate) fn class_items(c: &Cst, cls: NodeId) -> Vec<NodeId> {
    c.tree.children(cls).filter(|&x| !matches!(c.kind(x), Name | Header | EndLabel | Attribute)).collect()
}

/// A data declaration's qualifier tokens (before its type).
pub(crate) fn qualifiers<'t>(c: &Cst<'t>, decl: NodeId) -> Vec<&'t str> {
    let mut out = Vec::new();
    for k in c.kids(decl) {
        match *k {
            Child::Token(t) => out.push(c.text_of(t)),
            Child::Node(_) => break,
        }
    }
    out
}

/// Declarators of a declaration: their nodes and name tokens.
pub(crate) fn declarators(c: &Cst, decl: NodeId) -> Vec<(NodeId, String, u32)> {
    c.tree
        .children(decl)
        .filter(|&d| c.kind(d) == Declarator)
        .filter_map(|d| {
            let (n, t) = decl_name(c, d)?;
            Some((d, n, t))
        })
        .collect()
}

/// slang's `stripConversions`: casts (and parentheses, which the AST has
/// none of).
pub(crate) fn strip_casts(c: &Cst, mut e: NodeId) -> NodeId {
    loop {
        match c.kind(e) {
            ParenExpr => match c.tree.children(e).next() {
                Some(x) => e = x,
                None => return e,
            },
            CastExpr => match c.tree.children(e).nth(1) {
                Some(x) => e = x,
                None => return e,
            },
            _ => return e,
        }
    }
}

/// A call's invocation node and method name, as slang's
/// `getSubroutineName()`: `f()`, `o.f()`, `p::f()`, `o.f() with {...}`.
pub(crate) fn call_name<'t>(c: &Cst<'t>, call: NodeId) -> Option<&'t str> {
    if c.kind(call) != CallExpr {
        return None;
    }
    let callee = c.tree.children(call).next()?;
    match c.kind(callee) {
        CallExpr => call_name(c, callee),
        NameExpr | MemberExpr | ScopedExpr => Some(c.last_text(callee)),
        _ => None,
    }
}

/// The `with` clause of a call, if any.
pub(crate) fn with_clause(c: &Cst, call: NodeId) -> Option<NodeId> {
    c.tree.child(call, WithClause)
}

/// Code swe::UserCodeVisitor walks: not in package `uvm_pkg`, not in a
/// class named `uvm_*`, not from uvm_pkg.sv or uvm_mock.svh.
pub(crate) fn user_code(c: &Cst, n: NodeId) -> bool {
    let mut x = n;
    loop {
        match c.kind(x) {
            PackageDecl if decl_name(c, x).is_some_and(|(s, _)| s == "uvm_pkg") => return false,
            ClassDecl if decl_name(c, x).is_some_and(|(s, _)| s.starts_with("uvm_")) => return false,
            _ => {}
        }
        match c.parent(x) {
            Some(p) => x = p,
            None => break,
        }
    }
    true
}

/// Text between two tokens as written (their trivia between, not before).
pub(crate) fn span_text(c: &Cst, a: u32, b: u32) -> String {
    let mut s = String::new();
    for t in a..=b {
        let tok = &c.tree.tokens[t as usize];
        if t != a && !tok.from_macro {
            s.push_str(c.tree.trivia(t));
        }
        s.push_str(&tok.text);
    }
    s
}

/// Classes the UVM model indexes (not generic ones), with their items.
pub(crate) fn model_classes(c: &Cst) -> Vec<(NodeId, String)> {
    class_decls(c)
        .into_iter()
        .filter(|&k| !is_parameterized(c, k))
        .filter_map(|k| {
            let (name, _) = decl_name(c, k)?;
            // `class pc::inner` out of block names no class of its own.
            if c.tree.child(k, Header).is_some_and(|h| c.tok(h, "::").is_some()) {
                return None;
            }
            Some((k, name))
        })
        .collect()
}

/// slang's `ExpressionConstraintSyntax`: `[soft] expr ;` (an expression or
/// a dist), with its expression.
pub(crate) fn expression_constraint(c: &Cst, item: NodeId) -> Option<(bool, NodeId)> {
    let o = own(c, item);
    let soft = match o.as_slice() {
        [";"] => false,
        ["soft", ";"] => true,
        _ => return None,
    };
    let k = kids(c, item);
    match k.as_slice() {
        [e] if c.kind(*e) != ConstraintBlock && c.kind(*e) != ConstraintItem => Some((soft, *e)),
        _ => None,
    }
}
