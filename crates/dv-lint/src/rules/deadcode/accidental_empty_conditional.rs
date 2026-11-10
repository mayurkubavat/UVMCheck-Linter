//! SV-DED-003 (AccidentalEmptyConditional): Detects accidental semicolons
//! immediately following if() or while() headers.
//!
//! `if (c) ; else ...` is not reported: with an else the empty branch is a
//! deliberate idiom (an X or Z condition takes the else, as in OpenTitan's
//! `DV_CHECK`), and nothing runs unconditionally. The C++ rule is the same.

use crate::support::statements::{body_of, if_parts, in_bad_body, report, start, stmt, St, Units};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn empty_conditional(c: &Cst, s: &mut Sink) {
    let units = Units::new(c);
    for n in c.nodes(IfStmt) {
        if in_bad_body(c, n) {
            continue;
        }
        let (_, then, els) = if_parts(c, n);
        if els.is_none() && then.is_some_and(|t| c.kind(t) == EmptyStmt) {
            report(
                s,
                units.times(c, n),
                "SV-DED-003",
                "AccidentalEmptyConditional",
                Severity::Error,
                "Conditional statement 'if (...)' has an empty branch body (stray semicolon ';'). \
                 The subsequent block will execute unconditionally regardless of whether the condition evaluates true or false. \
                 Remove the accidental semicolon after the condition."
                    .into(),
                start(c, n),
            );
        }
    }
    for n in c.nodes(LoopStmt) {
        if stmt(c, n) != Some(St::While) || in_bad_body(c, n) {
            continue;
        }
        if body_of(c, n).is_some_and(|t| c.kind(t) == EmptyStmt) {
            report(
                s,
                units.times(c, n),
                "SV-DED-003",
                "AccidentalEmptyConditional",
                Severity::Error,
                "Loop statement 'while (...)' has an empty body (stray semicolon ';'). \
                 This creates either an unintended zero-delay infinite busy-wait loop or causes subsequent blocks to execute unconditionally."
                    .into(),
                start(c, n),
            );
        }
    }
}
