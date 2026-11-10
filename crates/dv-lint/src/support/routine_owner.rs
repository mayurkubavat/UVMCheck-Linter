//! Which class a routine belongs to, free routine names and library code.

use std::collections::HashSet;

use dv_model::{Class, Design};

use svlang::syntax::NodeId;

use crate::syntax::Cst;

use svlang::syntax::SyntaxKind::*;

/// Routine names a plain call can reach outside classes: free functions
/// and tasks, and DPI imports.
pub(crate) fn free_routine_names(csts: &[Cst]) -> HashSet<String> {
    let mut out = HashSet::new();
    for c in csts {
        for n in c.nodes_of(&[FunctionDecl, TaskDecl, DpiImport]) {
            let mut p = c.parent(n);
            let mut in_class = false;
            while let Some(x) = p {
                in_class |= c.kind(x) == ClassDecl;
                p = c.parent(x);
            }
            let Some(name) = c.tree.child(n, Name).map(|x| c.tree.bare_text(x)) else { continue };
            if !in_class && !name.contains("::") {
                out.insert(name);
            }
        }
    }
    out
}

/// The class of the routine that declares `decl`: the class around it, or
/// the one an out-of-block body names.
pub(crate) fn routine_class<'d>(d: &'d Design, c: &Cst, decl: NodeId) -> Option<&'d Class> {
    let mut p = c.parent(decl);
    while let Some(x) = p {
        if c.kind(x) == ClassDecl {
            let (_, nt) = c.tree.child(x, Name).and_then(|n| c.tree.token_bounds(n))?;
            let k = crate::support::class_roles::key(c.loc(nt));
            return d.classes().iter().find(|cls| crate::support::class_roles::key(cls.loc) == k);
        }
        p = c.parent(x);
    }
    let name = c.tree.child(decl, Name).map(|n| c.tree.bare_text(n))?;
    let (cls, _) = name.split_once("::")?;
    d.classes().iter().find(|k| k.name == cls)
}

/// A routine body slang binds as bad, so that its visitors skip it: it
/// holds a syntax error or an unknown macro, or (in a class) calls a plain
/// name that is neither a method of the class or a base nor a free routine.
pub(crate) fn bad_body(d: &Design, c: &Cst, body: NodeId, cls: Option<&Class>, free: &HashSet<String>) -> bool {
    let Some((a, b)) = c.tree.token_bounds(body) else { return false };
    if c.tree.errors.iter().any(|e| e.token >= a && e.token <= b) {
        return true;
    }
    let (ta, tb) = (&c.tree.tokens[a as usize], &c.tree.tokens[b as usize]);
    if !ta.from_macro && !tb.from_macro && ta.origin.file == tb.origin.file {
        let (f, lo, hi) = (ta.origin.file, ta.origin.offset, tb.origin.offset);
        if c.tree.pp_diagnostics.iter().any(|x| x.origin.file == f && x.origin.offset >= lo && x.origin.offset <= hi) {
            return true;
        }
    }
    let Some(cls) = cls else { return false };
    let known = |name: &str| {
        free.contains(name)
            || std::iter::once(cls).chain(d.ancestors(cls.id)).any(|k| k.methods.iter().any(|m| m.name == name))
    };
    c.subtree(body).into_iter().any(|n| {
        if c.kind(n) != CallExpr {
            return false;
        }
        let Some(callee) = c.tree.children(n).next() else { return false };
        if c.kind(callee) != NameExpr || c.tree.children(callee).next().is_some() {
            return false;
        }
        let name = c.tree.bare_text(callee);
        !name.starts_with('$') && name != "new" && !known(&name)
    })
}

/// Inside a library class or uvm_pkg, or written in a library file.
pub(crate) fn in_library(d: &Design, c: &Cst, n: NodeId) -> bool {
    let f = d.file_name(c.loc_of(n));
    if f.contains("uvm_mock.svh") || f.contains("uvm_pkg.sv") {
        return true;
    }
    let mut p = c.parent(n);
    while let Some(x) = p {
        if matches!(c.kind(x), ClassDecl | PackageDecl) {
            let name = c.tree.child(x, Name).map(|k| c.tree.bare_text(k)).unwrap_or_default();
            if (c.kind(x) == ClassDecl && name.starts_with("uvm_")) || name == "uvm_pkg" {
                return true;
            }
        }
        p = c.parent(x);
    }
    false
}
