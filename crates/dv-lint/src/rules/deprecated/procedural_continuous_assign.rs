//! SV-DEP-005 (ProceduralContinuousAssign): Disallow procedural 'assign' and
//! 'deassign' statements.

use sv_syntax::SyntaxKind;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::syntax::{Cst};

pub(crate) fn procedural_assign(c: &Cst, s: &mut Sink) {
    for n in c.nodes(ProcAssign) {
        // `deassign` and `release` are slang's ProceduralDeassignStatement.
        let Some(kw) = c.toks(n).find(|&t| matches!(c.text_of(t), "assign" | "force")) else {
            continue;
        };
        s.add(
            "SV-DEP-005",
            "ProceduralContinuousAssign",
            Severity::Error,
            format!(
                "Procedural continuous assignment ('{}') is non-synthesizable and strongly discouraged in IEEE 1800 § 10.6. \
                 Procedural assign/deassign statements create simulator-synthesis mismatches, break formal verification, \
                 and induce unpredictable dynamic driver overriding races. Use standard blocking/non-blocking procedural assignments.",
                c.text_of(kw)
            ),
            c.loc(kw),
        );
    }
}
