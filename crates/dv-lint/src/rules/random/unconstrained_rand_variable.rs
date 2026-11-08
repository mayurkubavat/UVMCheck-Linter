//! SV-RND-015 (UnconstrainedRandVariable): Flags non-static 'rand' class
//! properties that are never constrained in any constraint block within the
//! class.

use crate::support::class_items::{class_decls, class_items, decl_name, declarators, own, qualifiers, subtree};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::HashSet;
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

/// Whether a name chain (slang's ScopedName / IdentifierSelectName
/// pieces) starts here: names, selects of names, members of names.
fn is_name_chain(c: &Cst, n: NodeId) -> bool {
    match c.kind(n) {
        NameExpr | ScopedExpr => true,
        SelectExpr | MemberExpr => c.tree.children(n).next().is_some_and(|b| is_name_chain(c, b)),
        _ => false,
    }
}

fn is_identifier(t: &str) -> bool {
    let mut ch = t.chars();
    ch.next().is_some_and(|f| f.is_ascii_alphabetic() || f == '_' || f == '\\')
        && !matches!(
            t,
            "this"
                | "super"
                | "null"
                | "local"
                | "std"
                | "before"
                | "solve"
                | "foreach"
                | "inside"
                | "dist"
                | "soft"
                | "if"
                | "else"
                | "unique"
        )
}

/// slang's `IdentifierNameSyntax` identifiers below a node.
fn identifier_names(c: &Cst, root: NodeId, out: &mut HashSet<String>) {
    for n in subtree(c, root) {
        match c.kind(n) {
            NameExpr => {
                let o = own(c, n);
                let [t] = o.as_slice() else { continue };
                if !is_identifier(t) {
                    continue;
                }
                // `q[i]` is an IdentifierSelectName.
                let selected =
                    c.parent(n).is_some_and(|p| c.kind(p) == SelectExpr && c.tree.children(p).next() == Some(n));
                if !selected {
                    out.insert(t.to_string());
                }
            }
            MemberExpr | ScopedExpr => {
                // The member of a name chain is an IdentifierName unless
                // it is selected itself (`a.b[0]`).
                let base_ok = c.kind(n) == ScopedExpr || c.tree.children(n).next().is_some_and(|b| is_name_chain(c, b));
                let selected =
                    c.parent(n).is_some_and(|p| c.kind(p) == SelectExpr && c.tree.children(p).next() == Some(n));
                if base_ok && !selected {
                    if let Some(t) = c.toks(n).last() {
                        let x = c.text_of(t);
                        if is_identifier(x) {
                            out.insert(x.to_string());
                        }
                    }
                }
            }
            ConstraintItem => {
                // Names written as tokens: solve-before lists and foreach
                // headers.
                let toks: Vec<u32> = c.toks(n).collect();
                for (i, &t) in toks.iter().enumerate() {
                    let x = c.text_of(t);
                    if !is_identifier(x) {
                        continue;
                    }
                    let next = toks.get(i + 1).map(|&u| c.text_of(u));
                    let foreach = c.keyword(n).is_some_and(|k| c.text_of(k) == "foreach");
                    if next == Some("[") && !foreach {
                        continue;
                    }
                    out.insert(x.to_string());
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn unconstrained_rand(c: &Cst, s: &mut Sink) {
    for cls in class_decls(c) {
        let cname = decl_name(c, cls).map(|x| x.0).unwrap_or_default();
        let items = class_items(c, cls);
        let mut props: Vec<(String, u32)> = Vec::new();
        for &it in &items {
            if c.kind(it) != DataDecl || c.tree.child(it, DataType).is_none() {
                continue;
            }
            let q = qualifiers(c, it);
            if !q.contains(&"rand") || q.contains(&"static") {
                continue;
            }
            for (_, n, t) in declarators(c, it) {
                props.push((n, t));
            }
        }
        if props.is_empty() {
            continue;
        }
        let mut ids = HashSet::new();
        for &it in &items {
            if c.kind(it) == ConstraintDecl {
                identifier_names(c, it, &mut ids);
            }
        }
        for (p, at) in props {
            if ids.contains(&p) {
                continue;
            }
            s.add(
                "SV-RND-015",
                "UnconstrainedRandVariable",
                Severity::Warning,
                format!(
                    "Class property '{p}' in class '{cname}' is declared 'rand' but is never referenced in any constraint block. \
                     Unconstrained random properties generate uniformly unguided values across the entire bit-width, increasing solver cycles and frequently indicating forgotten stimulus specifications."
                ),
                c.loc(at),
            );
        }
    }
}
