//! SV-COR-020 (ProceduralAssignmentAsSubExpression): Disallows embedded
//! assignment expressions used as sub-expressions within conditionals or
//! arithmetic operations.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind::*;

pub(crate) fn assignment_sub_expr(c: &Cst, s: &mut Sink) {
    for n in c.nodes(BinaryExpr) {
        let Some(op) = c.tok(n, "=").or_else(|| c.tok(n, "+=")).or_else(|| c.tok(n, "-=")) else { continue };
        let mut up = c.parent(n);
        while let Some(p) = up.filter(|&p| c.kind(p) == ParenExpr) {
            up = c.parent(p);
        }
        let standalone = up.is_some_and(|p| match c.kind(p) {
            // `assign a = b, c = d;` is a list of standalone assignments.
            ExprStmt | CoverageOption | ContinuousAssign => true,
            LoopStmt => c.keyword(p).is_some_and(|k| c.text_of(k) == "for"),
            _ => false,
        });
        if standalone {
            continue;
        }
        let text = c.text(n);
        let last = text.rsplit('\n').next().unwrap_or("");
        let expr = last.trim_matches([' ', '\t', '\r']);
        s.add(
            "SV-COR-020",
            "ProceduralAssignmentAsSubExpression",
            Severity::Warning,
            format!(
                "Assignment expression '{expr}' is embedded as a sub-expression. In SystemVerilog (IEEE 1800 § 10.4), \
                 assignments should stand alone as dedicated statements rather than being embedded in conditionals \
                 or arithmetic expressions, which creates subtle order-of-evaluation race bugs and severely degrades code maintainability."
            ),
            c.loc(op),
        );
    }
}
