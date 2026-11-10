//! SV-ASC-001 (AssociativeArrayMutatingRead): Detects direct read lookups on
//! associative arrays in expressions without checking exists().

use crate::support::procedural::start;
use crate::support::slang_ast::kids;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::HashSet;
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

/// Names declared anywhere in the compile with an unpacked dimension that
/// reads like an associative index (`[int]`, `[*]`; also `[4]`, as the
/// C++ rule's text test has it).
fn assoc_names(c: &Cst) -> std::rc::Rc<HashSet<String>> {
    thread_local! {
        static NAMES: crate::shared::CompileCache<HashSet<String>> = const { std::cell::RefCell::new(None) };
    }
    let key = c.compile_key();
    if let Some(v) = NAMES.with(|k| k.borrow().as_ref().filter(|e| e.0 == key).map(|e| e.1.clone())) {
        return v;
    }
    let mut out = HashSet::new();
    for t in c.all {
        for i in 0..t.nodes.len() {
            let d = NodeId(i as u32);
            if t.kind(d) != Declarator {
                continue;
            }
            let assoc = t.children(d).filter(|&k| t.kind(k) == Dim).any(|k| {
                let text = t.bare_text(k);
                let text = text.trim();
                text.len() >= 3
                    && text.starts_with('[')
                    && text.ends_with(']')
                    && !text.contains(':')
                    && !text.contains('$')
                    && !text[1..text.len() - 1].trim().is_empty()
            });
            if assoc {
                if let Some(nm) = t.child(d, Name) {
                    let name = t.bare_text(nm);
                    if !name.is_empty() {
                        out.insert(name);
                    }
                }
            }
        }
    }
    let v = std::rc::Rc::new(out);
    NAMES.with(|k| *k.borrow_mut() = Some((key, v.clone())));
    v
}

pub(crate) fn assoc_mutating_read(c: &Cst, out: &mut Sink) {
    let names = assoc_names(c);
    if names.is_empty() {
        return;
    }
    for e in c.nodes(BinaryExpr) {
        let op = c.keyword(e).map_or("", |k| c.text_of(k));
        if !matches!(op, "==" | "!=" | "===" | "!==" | "<" | "<=" | ">" | ">=") {
            continue;
        }
        if op == "<=" && c.parent(e).is_some_and(|p| c.kind(p) == ExprStmt) {
            continue;
        }
        for side in kids(c, e).into_iter().take(2) {
            // slang's `name[i]...`: an identifier with selectors.
            if c.kind(side) != SelectExpr {
                continue;
            }
            let mut base = side;
            while c.kind(base) == SelectExpr {
                match kids(c, base).first() {
                    Some(&b) => base = b,
                    None => break,
                }
            }
            if c.kind(base) != NameExpr || c.toks(base).count() != 1 || !kids(c, base).is_empty() {
                continue;
            }
            let name = c.tree.bare_text(base);
            if !names.contains(&name) {
                continue;
            }
            let Some(loc) = start(c, side) else { continue };
            out.add(
                "SV-ASC-001",
                "AssociativeArrayMutatingRead",
                Severity::Warning,
                format!(
                    "Direct read indexing on associative array '{name}' in relational comparison. In SystemVerilog (IEEE 1800 § 7.8.1), \
                     reading a non-existent key silently inserts the key and populates it with a default value, mutating array state and \
                     leaking memory. Guard the lookup with '{name}.exists(...)' before indexing."
                ),
                loc,
            );
        }
    }
}
