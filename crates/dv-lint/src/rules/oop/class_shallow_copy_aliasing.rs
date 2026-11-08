//! SV-OOP-006 (ClassShallowCopyAliasing): Using shallow copy (new other_obj) on
//! classes containing nested object handles creates shared references across
//! independent transactions.

use crate::support::class_items::{class_decls, class_items, decl_name, declarators, kids, qualifiers};
use crate::support::sema_facts::ast_ok;
use crate::support::type_text::{base_name, is_class_type, type_name};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::HashMap;
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

/// The class around `n`, also for an out-of-block method body.
fn enclosing_class(c: &Cst, n: NodeId, classes: &HashMap<String, NodeId>) -> Option<NodeId> {
    let mut x = n;
    while let Some(p) = c.parent(x) {
        match c.kind(p) {
            ClassDecl => return Some(p),
            FunctionDecl | TaskDecl => {
                if let Some(nm) = c.tree.child(p, Name) {
                    let toks = c.tree.token_range(nm);
                    if toks.len() == 3 && c.text_of(toks[1]) == "::" {
                        return classes.get(c.text_of(toks[0])).copied();
                    }
                }
            }
            _ => {}
        }
        x = p;
    }
    None
}

/// A declarator without unpacked dimensions named `name` among `decl`s.
fn find_decl(c: &Cst, decls: impl Iterator<Item = NodeId>, name: &str) -> Option<NodeId> {
    for d in decls {
        if !matches!(c.kind(d), DataDecl | Port) || qualifiers(c, d).contains(&"typedef") {
            continue;
        }
        for (dcl, v, _) in declarators(c, d) {
            if v == name {
                if c.tree.children(dcl).any(|k| c.kind(k) == Dim) {
                    return None;
                }
                return c.tree.child(d, DataType);
            }
        }
    }
    None
}

/// Class properties of `cls` and its bases: their data types.
fn class_props(c: &Cst, cls: NodeId, classes: &HashMap<String, NodeId>) -> Vec<(String, Option<NodeId>)> {
    let mut out = Vec::new();
    let mut cur = Some(cls);
    let mut guard = 0;
    while let Some(k) = cur {
        for d in class_items(c, k).into_iter().filter(|&d| c.kind(d) == DataDecl) {
            if qualifiers(c, d).contains(&"typedef") {
                continue;
            }
            for (dcl, v, _) in declarators(c, d) {
                let dims = c.tree.children(dcl).any(|x| c.kind(x) == Dim);
                out.push((v, if dims { None } else { c.tree.child(d, DataType) }));
            }
        }
        guard += 1;
        cur = if guard < 64 { base_name(c, k).and_then(|b| classes.get(&b).copied()) } else { None };
    }
    out
}

/// The class a copy's source names: `this`, or a variable of class type.
fn source_class(c: &Cst, src: NodeId, classes: &HashMap<String, NodeId>) -> Option<NodeId> {
    if c.kind(src) != NameExpr {
        return None;
    }
    let name = c.tree.bare_text(src);
    if name == "this" {
        return enclosing_class(c, src, classes);
    }
    let mut x = src;
    while let Some(p) = c.parent(x) {
        let found = match c.kind(p) {
            Body | SeqBlock | ModuleDecl | PackageDecl | ProgramDecl | InterfaceDecl => {
                find_decl(c, c.tree.children(p), &name)
            }
            FunctionDecl | TaskDecl => {
                find_decl(c, c.tree.child(p, PortList).into_iter().flat_map(|l| c.tree.children(l)), &name)
            }
            ClassDecl => class_props(c, p, classes).into_iter().find(|(v, _)| *v == name).and_then(|(_, d)| d),
            _ => None,
        };
        if let Some(dt) = found {
            return type_name(c, dt).and_then(|n| classes.get(&n).copied());
        }
        if matches!(c.kind(p), FunctionDecl | TaskDecl) {
            if let Some(cls) = enclosing_class(c, x, classes).filter(|&k| c.parent(p) != Some(k)) {
                let dt = class_props(c, cls, classes).into_iter().find(|(v, _)| *v == name).and_then(|(_, d)| d);
                return dt.and_then(|d| type_name(c, d)).and_then(|n| classes.get(&n).copied());
            }
        }
        x = p;
    }
    None
}

pub(crate) fn shallow_copy(c: &Cst, s: &mut Sink) {
    let classes: HashMap<String, NodeId> = class_decls(c)
        .into_iter()
        .filter(|&k| !c.tree.child(k, Header).is_some_and(|h| c.tok(h, "::").is_some()))
        .filter_map(|k| Some((decl_name(c, k)?.0, k)))
        .collect();
    for n in c.nodes(NewExpr) {
        let k = kids(c, n);
        let [src] = k.as_slice() else { continue };
        if c.kind(*src) == ArgList {
            continue;
        }
        let Some(cls) = source_class(c, *src, &classes) else { continue };
        if !class_props(c, cls, &classes).iter().any(|(_, dt)| is_class_type(c, *dt, &classes)) {
            continue;
        }
        if !ast_ok(c, n) {
            continue;
        }
        let Some(at) = c.first_tok(n) else { continue };
        let cname = decl_name(c, cls).map(|x| x.0).unwrap_or_default();
        s.add(
            "SV-OOP-006",
            "ClassShallowCopyAliasing",
            Severity::Error,
            format!(
                "Shallow copy 'new {}' on class '{cname}' duplicates primitive fields but copies nested class handles by \
                 reference. Both instances will share the same nested object handles in memory, creating data corruption \
                 and race conditions. Implement a deep copy method (e.g. clone() or copy()) instead.",
                c.tree.text(*src)
            ),
            c.loc(at),
        );
    }
}
