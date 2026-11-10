//! Types and widths that depend on parameters: a fact about one
//! specialization is not a fact about the code as written.
//!
//! `idx_t'(sel)` with `idx_t = logic [IdxWidth-1:0]` is redundant only
//! where IdxWidth is 1; `cnt == 0` compares a 1-bit value only where the
//! counter's width parameter is 1. Rules that judge the code (SV-TYP-008,
//! SV-RDB-005) leave such cases alone, in both engines.

use crate::support::slang_ast::{ancestors, kids, lookup, DeclKind};
use crate::syntax::Cst;
use svlang::lexer::Kind;
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

/// Some name in `n` (a type, or a size) at `at` is a parameter, a type
/// parameter, or a typedef whose definition depends on one.
pub(crate) fn depends_on_params(c: &Cst, at: NodeId, n: NodeId) -> bool {
    depends(c, at, n, 0)
}

fn depends(c: &Cst, at: NodeId, n: NodeId, depth: usize) -> bool {
    if depth > 8 {
        return true;
    }
    c.tree.token_range(n).into_iter().any(|t| {
        if c.tree.tokens[t as usize].kind != Kind::Identifier {
            return false;
        }
        let name = c.text_of(t);
        if let Some(d) = lookup(c, at, name) {
            return d.kind == DeclKind::Parameter;
        }
        match typedef_named(c, at, name) {
            Some(td) => c.tree.child(td, DataType).is_some_and(|dt| depends(c, td, dt, depth + 1)),
            None => false,
        }
    })
}

/// The nearest `typedef ... name;` in a scope around `at`.
fn typedef_named(c: &Cst, at: NodeId, name: &str) -> Option<NodeId> {
    ancestors(c, at).find_map(|a| {
        kids(c, a)
            .into_iter()
            .find(|&k| c.kind(k) == TypedefDecl && c.tree.child(k, Name).is_some_and(|nm| c.tree.bare_text(nm) == name))
    })
}

/// `e` is a name declared with exactly the type written as `ty` (one name:
/// `data_t x;` against `data_t'(x)`).
pub(crate) fn declared_as(c: &Cst, e: NodeId, ty: NodeId) -> bool {
    if c.kind(e) != NameExpr || c.kind(ty) != NameExpr {
        return false;
    }
    let Some(t) = c.first_tok(e) else { return false };
    let Some(d) = lookup(c, e, c.text_of(t)) else { return false };
    d.dtype.is_some_and(|dt| c.tree.bare_text(dt) == c.tree.bare_text(ty))
}

/// The packed dimensions of the type `e` (a name, or a select of one) is
/// declared with depend on parameters.
pub(crate) fn width_depends_on_params(c: &Cst, e: NodeId) -> bool {
    let mut base = e;
    while c.kind(base) == SelectExpr {
        match kids(c, base).first() {
            Some(&b) => base = b,
            None => return false,
        }
    }
    if c.kind(base) != NameExpr {
        return false;
    }
    let Some(t) = c.first_tok(base) else { return false };
    let Some(d) = lookup(c, base, c.text_of(t)) else { return false };
    // A select of the variable (`v[0]`) is one bit whatever its width.
    base == e && d.dtype.is_some_and(|dt| depends_on_params(c, dt, dt))
}
