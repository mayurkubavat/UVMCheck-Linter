//! SV-SML-004 (CyclomaticComplexityGodMethod): Detects subroutines with high
//! McCabe Cyclomatic Complexity ($M > 15$).

use crate::support::routine_metrics::visited;
use crate::support::statements::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

fn cyclomatic_simple(c: &Cst, body: NodeId) -> i64 {
    let mut n = 1;
    for x in visited(c, body) {
        match c.kind(x) {
            IfStmt => n += 1,
            CaseStmt => {
                if stmt(c, x) == Some(St::Case) {
                    n += case_parts(c, x).items.len() as i64;
                }
            }
            LoopStmt => {
                if matches!(stmt(c, x), Some(St::For | St::Foreach | St::While | St::Repeat)) {
                    n += 1;
                }
            }
            DoWhileStmt | CondExpr => n += 1,
            BinaryExpr => {
                if matches!(op(c, x), "&&" | "||") {
                    n += 1;
                }
            }
            _ => {}
        }
    }
    n
}

pub(crate) fn god_method(c: &Cst, s: &mut Sink) {
    let units = Units::new(c);
    let max = 15;
    let warn = 25;
    for d in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)) {
        let Some(body) = c.tree.child(d, Body) else { continue };
        let Some(nm) = c.tree.child(d, Name) else { continue };
        let Some((_, tok)) = c.tree.token_bounds(nm) else {
            continue;
        };
        let n = cyclomatic_simple(c, body);
        if n <= max {
            continue;
        }
        let sev = if n > warn { Severity::Warning } else { Severity::Info };
        report(
            s,
            units.times(c, d),
            "SV-SML-004",
            "CyclomaticComplexityGodMethod",
            sev,
            format!(
                "Subroutine '{}' has high McCabe Cyclomatic Complexity ({n} > {max}). Overly complex routines with deeply nested branching impede functional coverage, \
                 conceal unreachable edge cases, and lead to high defect density during maintenance. \
                 Decompose into smaller, modular sub-tasks.",
                c.text_of(tok)
            ),
            c.loc(tok),
        );
    }
}
