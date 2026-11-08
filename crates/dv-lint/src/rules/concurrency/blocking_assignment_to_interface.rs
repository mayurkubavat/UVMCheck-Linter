//! SV-INT-001 (BlockingAssignmentToInterface): Disallows blocking assignments
//! ('=') when driving virtual interface or clocking block signals from
//! procedural code.

use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn blocking_to_interface(c: &Cst, s: &mut Sink) {
    for b in c.nodes(BinaryExpr) {
        let Some(op) = c.keyword(b) else { continue };
        if c.text_of(op) != "=" {
            continue;
        }
        // A continuous assignment drives the signal outside any process, so
        // there is no Active-region race to avoid, and `assign x <= y` is not
        // legal SystemVerilog.
        if crate::support::slang_ast::ancestors(c, b).any(|a| c.kind(a) == ContinuousAssign) {
            continue;
        }
        let Some(&l) = kids(c, b).first() else { continue };
        let lhs = slang_trimmed(c, l);
        if lhs.contains(".cb.") || lhs.starts_with("cb.") || lhs.contains("vif.") || lhs.contains("_vif.") {
            s.add(
                "SV-INT-001",
                "BlockingAssignmentToInterface",
                Severity::Error,
                format!(
                    "Blocking assignment '=' used to drive clocking block or interface signal '{lhs}'. In SystemVerilog (IEEE 1800 § 14.16), clocking block and interface signals must be driven using non-blocking clocking drive ('<=') to avoid simulation race conditions with DUT Active-region logic."
                ),
                c.loc(op),
            )
            .fixes
            .extend({
                // A function may not hold '<=' cleanly either (SV-LANG-005
                // would undo the fix), so there the rewrite is only shown.
                let in_function = crate::support::slang_ast::ancestors(c, b).any(|a| c.kind(a) == FunctionDecl);
                let (desc, safety) = if in_function {
                    ("Move the drive into a task and use non-blocking '<='", crate::fix::FixSafety::DisplayOnly)
                } else {
                    ("Replace blocking assignment '=' with non-blocking clocking drive '<='", crate::fix::FixSafety::Unsafe)
                };
                crate::fix::with_safety(c.token_fix(op, op, "<=", desc), safety)
            });
        }
    }
}
