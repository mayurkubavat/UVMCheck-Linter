//! SV-CON-005 (SemaphoreZeroKey): Flags semaphore method calls (sem.get(0),
//! sem.put(0), sem.try_get(0)) passing non-positive key counts (<= 0).

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::{Child, NodeId, SyntaxKind};
use SyntaxKind::*;

/// The first argument of an argument list when it is ordered (not named or
/// left empty): its expression.
fn first_ordered_arg(c: &Cst, list: NodeId) -> Option<NodeId> {
    let kids = c.kids(list);
    let open = kids.iter().position(|k| matches!(*k, Child::Token(t) if c.text_of(t) == "("))?;
    let Child::Node(arg) = *kids.get(open + 1)? else { return None };
    if c.kind(arg) != Arg || c.keyword(arg).is_some_and(|t| c.text_of(t) == ".") {
        return None;
    }
    c.items(arg).into_iter().next()
}

pub(crate) fn semaphore_zero_key(c: &Cst, s: &mut Sink) {
    for n in c.nodes(CallExpr) {
        let Some(left) = c.tree.children(n).next() else { continue };
        let method = c.last_text(left);
        if !matches!(method, "get" | "put" | "try_get") || !c.text(left).contains("sem") {
            continue;
        }
        let Some(args) = c.tree.child(n, ArgList) else { continue };
        let Some(expr) = first_ordered_arg(c, args) else { continue };
        let arg = c.trimmed(expr);
        if matches!(arg.as_str(), "0" | "32'd0" | "1'b0") || arg.starts_with('-') {
            let Some(first) = c.first_tok(expr) else { continue };
            s.add(
                "SV-CON-005",
                "SemaphoreZeroKey",
                Severity::Error,
                format!(
                    "Semaphore operation '.{method}({arg})' specifies a non-positive key count (<= 0). Per IEEE 1800 § 15.3, key counts must be positive integers >= 1. Requesting 0 keys causes immediate simulation deadlock or undefined simulator scheduling behavior."
                ),
                c.loc(first),
            );
        }
    }
}
