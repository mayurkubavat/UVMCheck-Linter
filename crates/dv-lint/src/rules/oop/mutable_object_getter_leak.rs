//! SV-ENC-001 (MutableObjectGetterLeak): Disallows getter functions (get_*())
//! from returning direct references to mutable internal class object members.

use crate::support::class_items::{class_decls, declarators, own};
use crate::support::data_items::{data_items, methods, routine_name};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::HashSet;
use sv_syntax::SyntaxKind::*;

const PRIMITIVE: &[&str] = &[
    "int",
    "integer",
    "bit",
    "logic",
    "reg",
    "byte",
    "shortint",
    "longint",
    "time",
    "real",
    "shortreal",
    "realtime",
    "string",
    "void",
    "chandle",
    "event",
];

pub(crate) fn getter_leak(c: &Cst, s: &mut Sink) {
    for cls in class_decls(c) {
        let mut objects: HashSet<String> = HashSet::new();
        for d in data_items(c, cls) {
            let ty = c.trimmed(c.tree.child(d, DataType).unwrap());
            if !PRIMITIVE.contains(&ty.as_str()) {
                for (_, n, _) in declarators(c, d) {
                    objects.insert(n);
                }
            }
        }
        for f in methods(c, cls) {
            let name = routine_name(c, f);
            let b = name.as_bytes();
            if !(name.starts_with("get_") || (name.starts_with("get") && b.len() > 3 && b[3].is_ascii_uppercase())) {
                continue;
            }
            for r in crate::support::class_items::subtree(c, f) {
                if c.kind(r) != ReturnStmt {
                    continue;
                }
                let Some(v) = c.tree.children(r).next() else { continue };
                if c.kind(v) != NameExpr {
                    continue;
                }
                let o = own(c, v);
                let [var] = o.as_slice() else { continue };
                if !objects.contains(*var) {
                    continue;
                }
                let Some(kw) = c.tok(r, "return") else { continue };
                s.add(
                    "SV-ENC-001",
                    "MutableObjectGetterLeak",
                    Severity::Warning,
                    format!(
                        "Getter method '{name}()' returns direct reference to mutable member object '{var}'. \
                         Returning handles to internal mutable class objects breaks OOP encapsulation and allows callers to mutate internal state without ownership control. \
                         Return a cloned copy of the object (e.g. 'return {var}.clone();') or ensure the object is immutable."
                    ),
                    c.loc(kw),
                );
            }
        }
    }
}
