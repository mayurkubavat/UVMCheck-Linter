//! SV-LANG-005 (NonBlockingInFunction): Prohibits non-blocking assignments
//! ('<=') inside functions.
//!
//! IEEE 1800 13.4.4 allows them: a function only may not block. What goes
//! wrong is the caller's view: the update lands after the caller moves on,
//! so code right after the call reads the old value. A drive of a clocking
//! block or virtual interface signal is the exception, and the form SV-INT-001
//! asks for (a driver's `reset_signals()` function): not reported. The C++
//! rule is the same.

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
            let lhs =
                c.tree.children(n).next().map(|l| crate::support::slang_ast::slang_trimmed(c, l)).unwrap_or_default();
            if crate::rules::concurrency::blocking_assignment_to_interface::interface_signal(&lhs) {
                continue;
            }
            s.add(
                "SV-LANG-005",
                "NonBlockingInFunction",
                Severity::Error,
                format!(
                    "Non-blocking assignment '<=' used inside function '{name}'. A function runs in zero time, but a non-blocking update lands in the NBA region after its caller has moved on (IEEE 1800 § 13.4.4 allows it), so code right after the call still reads the old value. Use '=' for variables; '<=' belongs on interface and clocking block signals."
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
