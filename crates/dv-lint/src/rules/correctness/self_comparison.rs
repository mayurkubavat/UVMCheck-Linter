//! SV-COR-008 (SelfComparison): Disallows self-comparisons where an operand is
//! compared to itself (e.g. a == a), which tautologically evaluates to constant
//! true or false. Not two equal literals (`"" == ""`), which is what a macro
//! argument's default expands to. The C++ rule is the same.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind;
use SyntaxKind::*;

/// A lone literal: a quoted string, or a number (`0`, `8'hFF`, `'1`, `1.5`).
/// Two equal literals are a constant test, not a variable compared to
/// itself: it is what a macro argument's default leaves behind
/// (`if (NAME == "")` with `NAME=""`, as UVM's compare macros do).
pub(crate) fn lone_literal(text: &str) -> bool {
    if let Some(inner) = text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
        return !inner.replace("\\\\", "").replace("\\\"", "").contains('"');
    }
    text.starts_with(|ch: char| ch.is_ascii_digit() || ch == '\'')
        && text.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '\'' | '.'))
}

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
        if !ls.is_empty() && ls == rs && !lone_literal(&ls) {
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
