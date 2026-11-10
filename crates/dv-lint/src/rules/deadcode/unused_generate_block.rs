//! SV-DED-008 (UnusedGenerateBlock): Flags conditional statements or generate
//! blocks with constant false conditions ('0') and no 'else' clause.

use crate::support::statements::{if_parts, start};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn false_condition(c: &Cst, s: &mut Sink) {
    for n in c.nodes(IfStmt) {
        let (cond, _, els) = if_parts(c, n);
        if els.is_some() || c.tree.child(n, ElseClause).is_some() {
            continue;
        }
        let Some(cond) = cond else { continue };
        if c.kind(cond) == MatchExpr {
            continue;
        }
        let text = c.trimmed(cond);
        if matches!(text.as_str(), "0" | "1'b0" | "32'd0" | "false") {
            s.add(
                "SV-DED-008",
                "UnusedGenerateBlock",
                Severity::Info,
                format!(
                    "Conditional statement specifies compile-time false condition '{text}' with no 'else' fallback. \
                     The enclosed code can never be reached or executed under any simulation conditions."
                ),
                start(c, cond),
            );
        }
    }
}
