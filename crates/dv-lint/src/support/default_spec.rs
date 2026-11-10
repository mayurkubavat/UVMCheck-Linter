//! Routine bodies that fail to bind in a generic class's default
//! specialization.
//!
//! slang walks the default specialization of every generic class (see
//! [`crate::support::specializations`]), binding its code with each type
//! parameter at its default. When that default is a built-in type (`type
//! IMP = int`) and the body uses a value of that type as a class handle
//! (`x == null`, `x = null`, `x.put(t)`), the statement fails to bind, and
//! slang marks the whole body bad: an `AllGood` AST walk skips it in that
//! specialization (and only that one). UVM's `uvm_*_master_imp` and
//! `uvm_*_slave_imp` constructors are the common case.
//!
//! [`default_bad_bodies`] lists those bodies (function and task declaration
//! nodes); [`crate::support::statements::Units::spec_times`] counts one
//! specialization fewer for code inside them.

use std::collections::HashSet;

use svlang::syntax::{NodeId, SyntaxKind};

use crate::syntax::Cst;
use SyntaxKind::*;

/// Built-in types a class handle operation rejects (`chandle` and `event`
/// take `null`).
fn non_class_builtin(t: &str) -> bool {
    matches!(
        t,
        "int" | "bit" | "logic" | "reg" | "byte" | "shortint" | "longint" | "integer" | "string" | "real" | "shortreal"
    )
}

/// A data type written as one name (`T`, `int`).
fn single_name(c: &Cst, dt: NodeId) -> Option<String> {
    let toks: Vec<u32> = c.toks(dt).collect();
    if c.tree.children(dt).next().is_some() {
        return None;
    }
    match toks[..] {
        [t] => Some(c.text_of(t).to_string()),
        _ => None,
    }
}

/// The function and task declarations whose bodies fail to bind in the
/// default specialization of their generic class.
pub(crate) fn default_bad_bodies(c: &Cst) -> HashSet<u32> {
    let mut out = HashSet::new();
    for cls in c.nodes(ClassDecl) {
        let Some(pl) = c.tree.child(cls, Header).and_then(|h| c.tree.child(h, ParamList)) else { continue };
        // Type parameters with their defaults; a parameter without one
        // means there is no default specialization.
        let mut params: Vec<(String, Option<String>)> = Vec::new();
        let mut complete = true;
        for p in c.tree.children(pl).filter(|&x| c.kind(x) == ParamDecl) {
            if c.tree.child(p, Declarator).is_some() {
                // A value parameter: needs a default too.
                let has = c.tree.child(p, Declarator).is_some_and(|d| c.tree.children(d).any(|x| c.kind(x) != Name));
                complete &= has;
                continue;
            }
            let Some(nm) = c.tree.child(p, Name) else { continue };
            let default = c.tree.children(p).find(|&x| x != nm);
            complete &= default.is_some();
            params.push((c.tree.bare_text(nm), default.and_then(|d| single_name(c, d))));
        }
        if !complete || params.is_empty() {
            continue;
        }
        // Names standing for a built-in type in the default specialization:
        // parameters, parameters defaulting to those, class typedefs of them.
        // Each such name with the built-in type it stands for.
        let mut builtin: std::collections::HashMap<String, String> = Default::default();
        let typedefs: Vec<(String, String)> = c
            .tree
            .children(cls)
            .filter(|&x| c.kind(x) == TypedefDecl)
            .filter_map(|t| {
                let nm = c.tree.child(t, Name)?;
                let ty = single_name(c, c.tree.child(t, DataType)?)?;
                Some((c.tree.bare_text(nm), ty))
            })
            .collect();
        loop {
            let before = builtin.len();
            for (n, d) in &params {
                let Some(d) = d.as_deref() else { continue };
                let base = if non_class_builtin(d) { Some(d.to_string()) } else { builtin.get(d).cloned() };
                if let Some(b) = base {
                    builtin.entry(n.clone()).or_insert(b);
                }
            }
            for (n, t) in &typedefs {
                if let Some(b) = builtin.get(t).cloned() {
                    builtin.entry(n.clone()).or_insert(b);
                }
            }
            if builtin.len() == before {
                break;
            }
        }
        if builtin.is_empty() {
            continue;
        }
        // A built-in default used as a scope (`T::type_id`) fails to bind
        // too; slang still builds the default specialization and drops only
        // the body around it.
        let scoped = |body: NodeId| {
            let toks = c.tree.token_range(body);
            toks.windows(2).any(|w| builtin.contains_key(c.text_of(w[0])) && c.text_of(w[1]) == "::")
        };
        // Names the class itself uses as a scope elsewhere (`T::type_id`):
        // meant to be classes. Passing a value of one to a routine fails to
        // bind in the default specialization too (the formal is a class).
        let ctoks = c.tree.token_range(cls);
        let class_meant: std::collections::HashSet<&str> = ctoks
            .windows(2)
            .filter(|w| builtin.contains_key(c.text_of(w[0])) && c.text_of(w[1]) == "::")
            .map(|w| c.text_of(w[0]))
            .collect();
        let meant_vars = |owner: NodeId| -> Vec<String> {
            let mut v = Vec::new();
            for d in c.tree.children(owner).filter(|&x| matches!(c.kind(x), DataDecl | Port)) {
                let Some(ty) = c.tree.child(d, DataType).and_then(|t| single_name(c, t)) else { continue };
                if !class_meant.contains(ty.as_str()) {
                    continue;
                }
                for dcl in c.tree.children(d).filter(|&x| c.kind(x) == Declarator && c.tree.child(x, Dim).is_none()) {
                    if let Some(nm) = c.tree.child(dcl, Name) {
                        v.push(c.tree.bare_text(nm));
                    }
                }
            }
            v
        };
        // Variables declared in `owner` with such a type, with the built-in.
        let typed_vars = |owner: NodeId| -> std::collections::HashMap<String, String> {
            let mut v = std::collections::HashMap::new();
            for d in c.tree.children(owner).filter(|&x| matches!(c.kind(x), DataDecl | Port)) {
                let Some(ty) = c.tree.child(d, DataType).and_then(|t| single_name(c, t)) else { continue };
                let Some(b) = builtin.get(&ty) else { continue };
                for dcl in c.tree.children(d).filter(|&x| c.kind(x) == Declarator) {
                    // An unpacked array of them is not one.
                    if c.tree.child(dcl, Dim).is_some() {
                        continue;
                    }
                    if let Some(nm) = c.tree.child(dcl, Name) {
                        v.insert(c.tree.bare_text(nm), b.clone());
                    }
                }
            }
            v
        };
        let members = typed_vars(cls);
        let class_name = c.tree.child(cls, Name).map(|n| c.tree.bare_text(n));
        let mut routines: Vec<NodeId> =
            c.tree.children(cls).filter(|&x| matches!(c.kind(x), FunctionDecl | TaskDecl)).collect();
        // Out-of-block bodies `C::f`.
        if let Some(cn) = &class_name {
            for d in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)) {
                let Some(nm) = c.tree.child(d, Name) else { continue };
                let t: Vec<u32> = c.toks(nm).collect();
                if t.len() >= 3 && c.text_of(t[0]) == cn && c.text_of(t[1]) == "::" {
                    routines.push(d);
                }
            }
        }
        for r in routines {
            let Some(body) = c.tree.child(r, Body) else { continue };
            if scoped(body) {
                out.insert(r.0);
                continue;
            }
            let mut vars = members.clone();
            if let Some(ports) = c.tree.child(r, PortList) {
                vars.extend(typed_vars(ports));
            }
            for n in crate::support::slang_ast::subtree(c, body) {
                if matches!(c.kind(n), Body | SeqBlock) {
                    vars.extend(typed_vars(n));
                }
            }
            if vars.is_empty() {
                continue;
            }
            let mut meant: Vec<String> = meant_vars(cls);
            if let Some(ports) = c.tree.child(r, PortList) {
                meant.extend(meant_vars(ports));
            }
            for n in crate::support::slang_ast::subtree(c, body) {
                if matches!(c.kind(n), Body | SeqBlock) {
                    meant.extend(meant_vars(n));
                }
            }
            let passed = |n: NodeId| {
                c.kind(n) == NameExpr
                    && meant.contains(&c.tree.bare_text(n))
                    && c.parent(n)
                        .filter(|&a| c.kind(a) == Arg)
                        .and_then(|a| c.parent(a))
                        .is_some_and(|l| c.kind(l) == ArgList && c.parent(l).is_some_and(|k| c.kind(k) == CallExpr))
            };
            let var_type = |n: NodeId| {
                if c.kind(n) == NameExpr {
                    vars.get(&c.tree.bare_text(n)).map(String::as_str)
                } else {
                    None
                }
            };
            let is_var = |n: NodeId| var_type(n).is_some();
            let is_null = |n: NodeId| c.kind(n) == NameExpr && c.tree.bare_text(n) == "null";
            let bad = crate::support::slang_ast::subtree(c, body).into_iter().any(|n| match c.kind(n) {
                BinaryExpr => {
                    let op = c.tree.children(n).count() == 2
                        && c.toks(n).any(|t| matches!(c.text_of(t), "==" | "!=" | "===" | "!==" | "="));
                    let k: Vec<NodeId> = c.tree.children(n).collect();
                    op && k.len() == 2 && ((is_var(k[0]) && is_null(k[1])) || (is_null(k[0]) && is_var(k[1])))
                }
                NameExpr => passed(n),
                // `x.f` on a value of a built-in type (strings have methods).
                MemberExpr => c.tree.children(n).next().and_then(var_type).is_some_and(|t| t != "string"),
                _ => false,
            });
            if bad {
                out.insert(r.0);
            }
        }
    }
    out
}
