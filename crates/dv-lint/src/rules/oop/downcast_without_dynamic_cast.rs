//! SV-OOP-011 (DowncastWithoutDynamicCast): Flags static casting syntax (e.g.

use crate::support::class_items::{decl_name, own};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use std::collections::HashSet;
use sv_syntax::SyntaxKind::*;

pub(crate) fn downcast(d: &Design, csts: &[Cst], s: &mut Sink) {
    let mut known: HashSet<String> =
        d.classes().iter().filter(|k| !k.is_parameterized).map(|k| k.name.clone()).collect();
    const HINTS: &[&str] =
        &["uvm_", "_item", "_seq", "_driver", "_monitor", "_scoreboard", "_env", "_component", "_object"];
    for c in csts {
        for n in c.tree.preorder() {
            match c.kind(n) {
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
                    s.add(
                        "SV-OOP-011",
                        "DowncastWithoutDynamicCast",
                        Severity::Error,
                        format!(
                            "Explicit static cast '{ty}'(...)' used on class handle. \
                             In SystemVerilog (IEEE 1800 § 8.16), downcasting class handles requires dynamic casting via \
                             '$cast(target, source)' to verify polymorphic type compatibility at runtime. \
                             Static class casting bypasses runtime type safety and causes simulator aborts. Replace with '$cast()'."
                        ),
                        c.loc(at),
                    );
                }
                _ => {}
            }
        }
    }
}
