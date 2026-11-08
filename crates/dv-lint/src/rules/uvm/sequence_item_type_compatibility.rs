//! UVM-SEQ-013 (SequenceItemTypeCompatibility): Statically verifies that
//! transaction item types between uvm_driver#(REQ) and uvm_sequencer#(REQ) are
//! type-compatible across connect_phase connections.
//! The item types are compared resolved, as slang compares types.

use crate::expr_types::expr_types;
use crate::support::config_db_calls::strip_parens;
use crate::support::slang_ast::kids;
use crate::support::slang_ast::DeclKind;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_sema::oracle::TypeOracle;
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

/// The leftmost name of a chain (`a` of `a.b.c`).
fn chain_root(c: &Cst, mut e: NodeId) -> NodeId {
    while matches!(c.kind(e), MemberExpr | ParenExpr) {
        match kids(c, e).first() {
            Some(&k) => e = k,
            None => break,
        }
    }
    e
}

/// The first type argument of the class an endpoint's value is, resolved
/// to the type it stands for (what slang compares): None when the value is
/// not a specialization or its first parameter is not a type.
fn item_type(c: &Cst, o: &TypeOracle, e: NodeId) -> Option<String> {
    let root = chain_root(c, e);
    let local = match c.kind(root) {
        NameExpr => lookup_local(c, root),
        _ => None,
    };
    let (_, args) = o.class_args(c.index(), e, local.map(|d| (c.index(), d)))?;
    args.into_iter().next().flatten()
}

/// The declared type of a local variable or argument a name refers to.
fn lookup_local(c: &Cst, n: NodeId) -> Option<NodeId> {
    let name = c.text_of(c.first_tok(n)?);
    let d = crate::support::slang_ast::lookup(c, n, name)?;
    matches!(d.kind, DeclKind::Local | DeclKind::Argument).then_some(d.dtype).flatten()
}

pub(crate) fn item_type_compatibility(c: &Cst, out: &mut Sink) {
    let types = expr_types(c);
    let ix = c.index();
    let mut oracle: Option<TypeOracle> = None;
    for f in c.nodes(FunctionDecl) {
        if crate::support::slang_ast::last_name(c, f) != "connect_phase" {
            continue;
        }
        // A class's method (in the class, or defined out of it).
        let in_class = c.parent(f).is_some_and(|p| c.kind(p) == ClassDecl) || c.toks(f).any(|t| c.text_of(t) == "::");
        if !in_class || crate::support::slang_ast::is_library(c, f) {
            continue;
        }
        // The UVM model's classes: not generic ones (their specializations
        // are not walked here).
        let owner = crate::support::slang_ast::ancestors(c, f).find(|&a| c.kind(a) == ClassDecl);
        if owner.is_some_and(|k| c.tree.child(k, Header).and_then(|h| c.tree.child(h, ParamList)).is_some()) {
            continue;
        }
        let mut todo = vec![f];
        while let Some(n) = todo.pop() {
            todo.extend(kids(c, n));
            if c.kind(n) != CallExpr {
                continue;
            }
            let Some((_, name, Some(recv))) = crate::support::slang_ast::call_parts(c, n) else { continue };
            if name != "connect" {
                continue;
            }
            let Some(&arg) = crate::expr_types::call_args(c, n).first() else { continue };
            let arg = strip_parens(c, arg);
            let (Some(rt), Some(at)) = (types.get(&(ix, recv)), types.get(&(ix, arg))) else { continue };
            // Compare the item types themselves: a specialization prints
            // with the parameter names of the code that made it first (`T`,
            // `REQ`), so one item type can print two ways.
            let o = oracle.get_or_insert_with(|| TypeOracle::new(c.all));
            let (Some(r1), Some(r2)) = (item_type(c, o, recv), item_type(c, o, arg)) else { continue };
            if r1 == r2 {
                continue;
            }
            let Some(t) = c.first_tok(n) else { continue };
            out.add(
                "UVM-SEQ-013",
                "SequenceItemTypeCompatibility",
                Severity::Warning,
                format!(
                    "Transaction item type mismatch in connect(): driver port type '{}' (REQ={r1}) is connected to sequencer export type '{}' (REQ={r2}). Mismatched item parameters will cause runtime slicing or dynamic cast failures.",
                    rt.text, at.text
                ),
                c.loc(t),
            );
        }
    }
}
