//! Free routines, assignment targets and the classes properties belong to.

use dv_model::{Class, Design};

use sv_syntax::NodeId;

use crate::syntax::Cst;

use sv_syntax::SyntaxKind::*;

/// Subroutines outside classes, in user code (`swe::freeRoutines`).
pub(crate) fn free_routines(d: &Design, c: &Cst) -> Vec<NodeId> {
    c.nodes_of(&[FunctionDecl, TaskDecl])
        .into_iter()
        .filter(|&n| {
            let mut p = c.parent(n);
            while let Some(x) = p {
                if c.kind(x) == ClassDecl {
                    return false;
                }
                p = c.parent(x);
            }
            let f = d.file_name(c.loc_of(n));
            !f.contains("uvm_mock.svh") && !f.contains("uvm_pkg.sv") && c.tree.child(n, Body).is_some()
        })
        .collect()
}

/// Assignments' left sides (`=`, compound ops, nonblocking `<=`).
pub(crate) fn assignment_lhs(c: &Cst, n: NodeId) -> Option<NodeId> {
    if c.kind(n) != BinaryExpr {
        return None;
    }
    let op = c.text_of(c.toks(n).next()?);
    let assign =
        matches!(op, "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=" | "<<<=" | ">>>=")
            || (op == "<=" && c.parent(n).is_some_and(|p| c.kind(p) == ExprStmt));
    if !assign {
        return None;
    }
    c.tree.children(n).next()
}

pub(crate) fn skip_selects(c: &Cst, mut n: NodeId) -> NodeId {
    loop {
        n = c.strip(n);
        if c.kind(n) != SelectExpr {
            return n;
        }
        match c.tree.children(n).next() {
            Some(x) => n = x,
            None => return n,
        }
    }
}

/// The class (or a base) whose property `name` is, if any.
pub(crate) fn property_owner<'d>(d: &'d Design, c: &'d Class, name: &str) -> Option<&'d Class> {
    std::iter::once(c).chain(d.ancestors(c.id)).find(|k| k.properties.iter().any(|p| p.name == name))
}
