//! SV-INT-001 (BlockingAssignmentToInterface): Disallows blocking assignments
//! ('=') when driving virtual interface or clocking block signals.

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
            );
        }
    }
}
