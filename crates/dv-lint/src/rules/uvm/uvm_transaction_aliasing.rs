//! UVM-ALS-001 (UvmTransactionAliasing): Disallows in-place mutation of
//! transaction objects after broadcasting via an analysis port (.write()).

use crate::support::uvm_syntax::{assignment, body_items, callee, first_ordered, stmt_expr};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

pub(crate) fn transaction_aliasing(c: &Cst, s: &mut Sink) {
    let mut lists: Vec<Vec<NodeId>> = Vec::new();
    for f in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)) {
        lists.push(body_items(c, f));
    }
    for b in c.nodes(SeqBlock).into_iter().chain(c.nodes(ParBlock)) {
        lists.push(c.items(b));
    }
    for items in lists {
        let mut written: std::collections::HashSet<String> = Default::default();
        for item in items {
            let Some(e) = stmt_expr(c, item) else { continue };
            if c.kind(e) == CallExpr && callee(c, e) == "write" {
                if let Some(arg) = first_ordered(c, e) {
                    let tr = c.last_text(arg);
                    if !tr.is_empty() && tr != "this" {
                        written.insert(tr.to_string());
                    }
                }
            }
            let Some((_, lhs, rhs)) = assignment(c, e) else { continue };
            let l = c.last_text(lhs).to_string();
            let r = c.text(rhs);
            if r.contains("new") || r.contains("clone") {
                written.remove(&l);
            }
            let base = match c.kind(lhs) {
                MemberExpr | ScopedExpr => c.tree.children(lhs).next().map(|x| c.last_text(x).to_string()),
                _ => None,
            };
            let Some(base) = base.filter(|b| !b.is_empty() && written.contains(b)) else { continue };
            let Some(at) = c.first_tok(lhs) else { continue };
            s.add(
                "UVM-ALS-001",
                "UvmTransactionAliasing",
                Severity::Warning,
                format!(
                    "Transaction handle '{base}' was broadcast to an analysis port ('write({base})') and subsequently mutated in-place without cloning. In UVM, analysis ports pass handles by reference; \
                     in-place mutations corrupt transactions already stored by downstream scoreboards and coverage collectors. \
                     Clone the transaction before writing or allocate a new instance with 'new()'."
                ),
                c.loc(at),
            );
            written.remove(&base);
        }
    }
}
