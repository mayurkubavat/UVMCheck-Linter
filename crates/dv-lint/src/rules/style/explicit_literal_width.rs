//! SV-STY-010 (ExplicitLiteralWidth): Disallow unsized integer and numeric
//! literals without explicit bit-width declarations.

use crate::support::literals::{ast_times, compact, integer_literal};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

/// A plain decimal's value is above one.
fn above_one(digits: &str) -> bool {
    let d: String = digits.chars().filter(|&ch| ch != '_').collect();
    let d = d.trim_start_matches('0');
    !(d.is_empty() || d == "1")
}

pub(crate) fn explicit_literal_width(c: &Cst, s: &mut Sink) {
    for n in c.nodes(Literal) {
        let Some(lit) = integer_literal(c, n) else { continue };
        if lit.sized || (!lit.based && !above_one(&lit.digits)) {
            continue;
        }
        let k = ast_times(c, n);
        if k == 0 {
            continue;
        }
        let outer = outer_paren(c, n);
        let mut text = compact(c, outer);
        // `#(50)`: slang keeps the parentheses as an expression.
        if c.parent(outer).is_some_and(|p| {
            c.kind(p) == TimingControl
                && c.tok(p, "#").is_some()
                && c.tok(p, "(").is_some()
                && c.tok(p, ":").is_none()
                && c.tok(p, ",").is_none()
        }) {
            text = format!("({text})");
        }
        let (sev, msg) = if lit.based {
            (
                Severity::Warning,
                format!(
                    "Numeric literal '{text}' is missing an explicit bit-width specification. By convention, all numeric literals must declare explicit width (e.g. 8'hFF instead of 'hFF) to avoid 32-bit truncation and sign extension traps."
                ),
            )
        } else {
            (
                Severity::Info,
                format!(
                    "Unsized decimal literal '{text}' lacks explicit bit-width specification. Specify explicit bit-width (e.g. 32'd{text}) to guarantee deterministic signedness and width sizing."
                ),
            )
        };
        let Some(at) = c.first_tok(n) else { continue };
        for _ in 0..k {
            s.add("SV-STY-010", "ExplicitLiteralWidth", sev, msg.clone(), c.loc(at));
        }
    }
}

/// The node slang keeps as a literal's syntax: the parentheses around it.
fn outer_paren(c: &Cst, mut n: NodeId) -> NodeId {
    // `(1:2:3)` is a min:typ:max expression, not a parenthesis.
    while let Some(p) = c.parent(n).filter(|&p| c.kind(p) == ParenExpr && c.tok(p, ":").is_none()) {
        n = p;
    }
    n
}
