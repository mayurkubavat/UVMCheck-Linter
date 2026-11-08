//! SV-COR-008 (SelfComparison): Disallows self-comparisons where an operand is
//! compared to itself (e.g. a == a), which tautologically evaluates to constant
//! true or false.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind;
use SyntaxKind::*;

/// slang's binary operator kinds the rule checks; `<=` directly under an
/// expression statement is a nonblocking assignment there instead.
pub(crate) fn self_comparison(c: &Cst, s: &mut Sink) {
    for n in c.nodes(BinaryExpr) {
        let Some(op) = c.toks(n).next() else { continue };
        let ops = ["==", "!=", "===", "!==", "==?", "!=?", "<", "<=", ">", ">="];
        if !ops.contains(&c.text_of(op)) {
            continue;
        }
        if c.text_of(op) == "<=" && c.parent(n).is_some_and(|p| c.kind(p) == ExprStmt) {
            continue;
        }
        let mut sides = c.tree.children(n);
        let (Some(l), Some(r)) = (sides.next(), sides.next()) else { continue };
        let (ls, rs) = (c.trimmed(l), c.trimmed(r));
        if !ls.is_empty() && ls == rs {
            s.add(
                "SV-COR-008",
                "SelfComparison",
                Severity::Error,
                format!(
                    "Self-comparison detected: expression '{ls}' is compared to itself using operator '{}'. Self-comparisons tautologically evaluate to constant true or false, almost invariably indicating a copy-paste error where another variable or expected value was intended.",
                    c.text_of(op)
                ),
                c.loc(op),
            );
        }
    }
}
