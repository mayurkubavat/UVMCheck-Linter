//! SV-CON-011 (WaitModifiedInLoop): Flags wait(...) statements inside loops
//! where condition variables are never modified or assigned inside the loop
//! body.

use crate::support::procedural::{assign_lhs, is_incdec, loop_kw};
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::HashSet;
use sv_syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

/// The identifiers slang keeps as `IdentifierName` under a node: names and
/// members, but not a name that a select follows (`a` of `a[i]`, which is
/// an `IdentifierSelectName`), in source order.
pub(crate) fn identifier_names(c: &Cst, n: NodeId) -> Vec<String> {
    let ident = |s: &str| {
        s.starts_with(|ch: char| ch.is_ascii_alphabetic() || ch == '_') && !matches!(s, "this" | "super" | "null")
    };
    let mut res: Vec<(u32, String)> = Vec::new();
    for x in subtree(c, n) {
        let selected = c.parent(x).is_some_and(|p| c.kind(p) == SelectExpr && kids(c, p).first() == Some(&x));
        if selected {
            continue;
        }
        let t = match c.kind(x) {
            NameExpr => c.first_tok(x),
            // The member is the last name token after the base.
            MemberExpr | ScopedExpr => c.toks(x).filter(|&t| ident(c.text_of(t))).last(),
            _ => None,
        };
        if let Some(t) = t {
            if ident(c.text_of(t)) {
                res.push((t, c.text_of(t).to_string()));
            }
        }
    }
    res.sort_by_key(|x| x.0);
    res.into_iter().map(|x| x.1).collect()
}

pub(crate) fn wait_in_loop(c: &Cst, s: &mut Sink) {
    for l in subtree(c, c.tree.root) {
        if !(c.kind(l) == DoWhileStmt || (c.kind(l) == LoopStmt && loop_kw(c, l) != "foreach")) {
            continue;
        }
        let all = subtree(c, l);
        let waits: Vec<NodeId> = all
            .iter()
            .copied()
            .filter(|&x| {
                c.kind(x) == WaitStmt
                    && c.keyword(x).is_some_and(|k| c.text_of(k) == "wait")
                    && !c.toks(x).any(|t| c.text_of(t) == "fork")
            })
            .collect();
        if waits.is_empty() {
            continue;
        }
        let mut modified: HashSet<String> = HashSet::new();
        for &x in &all {
            if let Some(lhs) = assign_lhs(c, x) {
                modified.extend(identifier_names(c, lhs));
            } else if let Some(o) = is_incdec(c, x) {
                modified.extend(identifier_names(c, o));
            }
        }
        for w in waits {
            let Some(cond) = kids(c, w).into_iter().find(|&x| !matches!(c.kind(x), StmtLabel | Attribute)) else {
                continue;
            };
            let ids = identifier_names(c, cond);
            if ids.is_empty() || ids.iter().any(|i| modified.contains(i)) {
                continue;
            }
            let Some(order) = std_unordered_order(&ids) else { continue };
            let list = order.iter().map(|i| format!("'{i}'")).collect::<Vec<_>>().join(", ");
            let Some(kw) = c.tok(w, "wait") else { continue };
            s.add(
                "SV-CON-011",
                "WaitModifiedInLoop",
                Severity::Warning,
                format!(
                    "Condition variables ({list}) in 'wait(...)' statement are not modified or assigned anywhere inside the loop body. \
                     In SystemVerilog concurrency guidelines (IEEE 1800 § 9.4.4), waiting inside a loop without mutating condition variables risks \
                     zero-delay infinite spinning or unblocked iteration once the condition becomes true."
                ),
                c.loc(kw),
            );
        }
    }
}
