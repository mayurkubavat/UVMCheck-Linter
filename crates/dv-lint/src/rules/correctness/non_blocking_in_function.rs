//! SV-LANG-005 (NonBlockingInFunction): Prohibits non-blocking assignments
//! ('<=') inside functions.

use crate::support::literals::compact;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn nonblocking_in_function(c: &Cst, s: &mut Sink) {
    for f in c.nodes(FunctionDecl) {
        let name = c.tree.child(f, Name).map(|x| compact(c, x)).unwrap_or_else(|| "function".to_string());
        for n in c.subtree(f) {
            if c.kind(n) != BinaryExpr || c.parent(n).map(|p| c.kind(p)) != Some(ExprStmt) {
                continue;
            }
            let Some(op) = c.tok(n, "<=") else { continue };
            s.add(
                "SV-LANG-005",
                "NonBlockingInFunction",
                Severity::Error,
                format!(
                    "Non-blocking assignment '<=' used inside function '{name}'. In SystemVerilog (IEEE 1800 § 13.4.4), functions execute in zero simulation time and must only use blocking assignments ('='). Non-blocking assignments schedule updates into the NBA region, violating function combinational execution semantics."
                ),
                c.loc(op),
            )
            .fixes
            .extend(crate::fix::with_safety(
                c.token_fix(op, op, "=", "Replace non-blocking assignment '<=' with blocking assignment '='"),
                crate::fix::FixSafety::Unsafe,
            ));
        }
    }
}
