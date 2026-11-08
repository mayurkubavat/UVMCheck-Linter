//! SV-RDB-004 (UndocumentedExtensionPoint): Flags new virtual methods without a
//! preceding comment in classes meant to be extended: classes with user
//! subclasses, classes named *base*, and callbacks (Checkstyle
//! MissingJavadocMethod, Pylint C0116).

use crate::support::class_docs::{decl_name, name_tok, own_comments, Classes, Role};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use std::collections::{HashMap, HashSet};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

fn is_uvm_standard_name(n: &str) -> bool {
    n == "new"
        || n.ends_with("_phase")
        || n.starts_with("do_")
        || matches!(
            n,
            "convert2string"
                | "get_type_name"
                | "create"
                | "body"
                | "pre_body"
                | "post_body"
                | "pre_start"
                | "post_start"
                | "write"
                | "catch"
        )
}

/// Methods of a class by name, with whether each is virtual.
fn method_virtuality(c: &Cst, cls: NodeId) -> HashMap<String, bool> {
    let mut out = HashMap::new();
    for m in c.tree.children(cls) {
        if matches!(c.kind(m), FunctionDecl | TaskDecl | MethodPrototype) {
            let v = c.toks(m).any(|t| c.text_of(t) == "virtual");
            out.insert(decl_name(c, m), v);
        }
    }
    out
}

pub(crate) fn undocumented_extension_point(_: &Design, csts: &[Cst], s: &mut Sink) {
    let classes = Classes::new(csts);
    let mut extended: HashSet<usize> = HashSet::new();
    for (i, _) in classes.user() {
        if let Some(b) = classes.base_of(i) {
            extended.insert(b);
        }
    }
    for (i, k) in classes.user() {
        let is_base = extended.contains(&i) || k.name.contains("base") || k.role == Role::Callback;
        if !is_base {
            continue;
        }
        let c = &csts[k.ix];
        // Virtual methods of the ancestors: an override is not a new
        // extension point.
        let mut inherited: HashMap<String, bool> = HashMap::new();
        let mut cur = classes.base_of(i);
        let mut guard = 0;
        while let Some(j) = cur {
            if guard > 64 {
                break;
            }
            guard += 1;
            let b = &classes.list[j];
            for (n, v) in method_virtuality(&csts[b.ix], b.node) {
                let e = inherited.entry(n).or_insert(false);
                *e |= v;
            }
            cur = classes.base_of(j);
        }
        for m in c.tree.children(k.node) {
            if !matches!(c.kind(m), FunctionDecl | TaskDecl | MethodPrototype) {
                continue;
            }
            let name = decl_name(c, m);
            let virt = c.toks(m).any(|t| matches!(c.text_of(t), "virtual" | "pure"));
            let overrides = inherited.get(&name).copied().unwrap_or(false);
            if !virt || overrides || is_uvm_standard_name(&name) {
                continue;
            }
            let Some(first) = c.first_tok(m) else { continue };
            if !own_comments(c, first).is_empty() {
                continue;
            }
            let Some(t) = name_tok(c, m) else { continue };
            s.add(
                "SV-RDB-004",
                "UndocumentedExtensionPoint",
                Severity::Info,
                format!(
                    "Virtual method '{}::{}' is an extension point with no comment. Write down when it is called and what \
                     an override must and must not do.",
                    k.name, name
                ),
                c.loc(t),
            );
        }
    }
}
