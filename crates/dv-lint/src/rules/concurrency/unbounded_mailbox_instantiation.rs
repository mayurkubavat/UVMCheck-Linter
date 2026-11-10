//! SV-CON-007 (UnboundedMailboxInstantiation): Disallows instantiating
//! unbounded mailboxes without a capacity limit to prevent memory bloat and
//! missed producer-consumer synchronization.

use crate::syntax::{assignments, Cst};
use crate::{Severity, Sink};
use svlang::syntax::{Child, NodeId, SyntaxKind};
use SyntaxKind::*;

/// slang's `NewClassExpression`: `new`, `new(...)` or `pkg::cls::new(...)`,
/// with its argument list.
fn new_class(c: &Cst, e: NodeId) -> Option<(u32, Option<NodeId>)> {
    match c.kind(e) {
        NewExpr => {
            let kw = c.tok(e, "new")?;
            if c.tok(e, "[").is_some() || c.items(e).into_iter().any(|x| c.kind(x) != ArgList) {
                return None;
            }
            Some((kw, c.tree.child(e, ArgList)))
        }
        CallExpr => {
            let callee = c.tree.children(e).next()?;
            (c.kind(callee) == ScopedExpr && c.last_text(callee) == "new")
                .then(|| Some((c.first_tok(callee)?, c.tree.child(e, ArgList))))?
        }
        _ => None,
    }
}

fn check_mailbox_new(c: &Cst, s: &mut Sink, e: NodeId, var: &str) {
    let Some((at, args)) = new_class(c, e) else { return };
    let unbounded = match args.map(|a| (a, c.items(a).into_iter().find(|&x| c.kind(x) == Arg))) {
        None | Some((_, None)) => true,
        Some((a, Some(_))) => {
            // slang's first parameter, which may be an empty argument.
            let kids = c.kids(a);
            let open = kids.iter().position(|k| matches!(*k, Child::Token(t) if c.text_of(t) == "("));
            match open.and_then(|i| kids.get(i + 1)) {
                Some(Child::Node(arg)) if c.kind(*arg) == Arg => {
                    let s = c.trimmed(*arg);
                    s == "0" || s == "32'd0"
                }
                _ => false,
            }
        }
    };
    if unbounded {
        s.add(
            "SV-CON-007",
            "UnboundedMailboxInstantiation",
            Severity::Warning,
            format!(
                "Mailbox '{var}' is instantiated with an unbounded queue limit (bound = 0 or omitted). Per IEEE 1800 § 15.4.1, unbounded mailboxes allow producers to allocate unlimited transaction memory without blocking, risking memory exhaustion and masking downstream testbench stalls. Specify a bounded capacity (e.g. 'new(16)')."
            ),
            c.loc(at),
        );
    }
}

pub(crate) fn unbounded_mailbox(c: &Cst, s: &mut Sink) {
    // Declarations and assignments are visited in source order, as slang's
    // visitor reaches them; findings are sorted afterwards anyway.
    for n in c.nodes(DataDecl) {
        let Some(ty) = c.tree.child(n, DataType) else { continue };
        if c.last_text(ty) != "mailbox" && !c.text(ty).contains("mailbox") {
            continue;
        }
        for d in c.tree.children(n).filter(|&d| c.kind(d) == Declarator) {
            let Some(eq) = c.tok(d, "=") else { continue };
            let Some(init) = c.node_after(d, eq) else { continue };
            let var = c.tree.child(d, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default();
            check_mailbox_new(c, s, init, &var);
        }
    }
    for (_, lhs, rhs) in assignments(c) {
        let left = c.last_text(lhs);
        if left.contains("mailbox") || left.ends_with("_mb") || left.ends_with("_mbox") || left.starts_with("mb_") {
            check_mailbox_new(c, s, rhs, left);
        }
    }
}
