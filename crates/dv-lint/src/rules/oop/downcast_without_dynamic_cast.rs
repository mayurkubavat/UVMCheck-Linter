//! SV-OOP-011 (DowncastWithoutDynamicCast): Flags static casting syntax (e.g.

use crate::support::class_items::{decl_name, own};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use std::collections::HashSet;
use svlang::syntax::SyntaxKind::*;

pub(crate) fn downcast(d: &Design, csts: &[Cst], s: &mut Sink) {
    let mut known: HashSet<String> =
        d.classes().iter().filter(|k| !k.is_parameterized).map(|k| k.name.clone()).collect();
    const HINTS: &[&str] =
        &["uvm_", "_item", "_seq", "_driver", "_monitor", "_scoreboard", "_env", "_component", "_object"];
    // A name only the hints match that a typedef declares (`uvm_reg_data_t`,
    // `uvm_radix_enum`) is a vector or enum type, not a class.
    let mut typedefs: HashSet<String> = HashSet::new();
    let mut pending = Vec::new();
    for c in csts {
        for n in c.tree.preorder() {
            match c.kind(n) {
                TypedefDecl => {
                    if let Some(nm) = c.tree.child(n, Name) {
                        typedefs.insert(c.tree.bare_text(nm));
                    }
                }
                ClassDecl => {
                    if let Some((name, _)) = decl_name(c, n) {
                        if !name.is_empty() {
                            known.insert(name);
                        }
                    }
                }
                CastExpr if own(c, n).contains(&"(") => {
                    let Some(left) = c.tree.children(n).next() else { continue };
                    // `void'(f());` as a statement is slang's
                    // VoidCastedCallStatement, not a cast expression.
                    if c.tree.bare_text(left) == "void" && c.parent(n).is_some_and(|p| c.kind(p) == ExprStmt) {
                        continue;
                    }
                    let ty = c.trimmed(left);
                    if !known.contains(&ty) && !HINTS.iter().any(|h| ty.contains(h)) {
                        continue;
                    }
                    let Some(at) = c.first_tok(n) else { continue };
                    pending.push((ty, c.loc(at)));
                }
                _ => {}
            }
        }
    }
    for (ty, loc) in pending {
        if !known.contains(&ty) && typedefs.contains(&ty) {
            continue;
        }
        s.add(
            "SV-OOP-011",
            "DowncastWithoutDynamicCast",
            Severity::Error,
            format!(
                "Explicit static cast '{ty}'(...)' used on class handle. \
                 In SystemVerilog (IEEE 1800 \u{a7} 8.16), downcasting class handles requires dynamic casting via \
                 '$cast(target, source)' to verify polymorphic type compatibility at runtime. \
                 Static class casting bypasses runtime type safety and causes simulator aborts. Replace with '$cast()'."
            ),
            loc,
        );
    }
}
