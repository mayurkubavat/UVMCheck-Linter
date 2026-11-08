//! SV-RND-021 (ConstraintInsideInvertedRange): Disallows inverted value ranges
//! [low : high] where low > high in set membership expressions.

use crate::support::spans::child_nodes;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

/// An unsized integer literal, read as `std::stoll(text, nullptr, 0)`
/// does: a leading 0 means octal, and parsing stops at the first `_`.
fn int_literal(text: &str) -> Option<i64> {
    if text.is_empty() || !text.chars().all(|ch| ch.is_ascii_digit() || ch == '_') {
        return None;
    }
    let (digits, radix) = match text.strip_prefix('0') {
        Some(rest) if !rest.is_empty() => (rest, 8),
        _ => (text, 10),
    };
    let end = digits.find(|ch: char| !ch.is_digit(radix)).unwrap_or(digits.len());
    if end == 0 {
        return (radix == 8).then_some(0);
    }
    i64::from_str_radix(&digits[..end], radix).ok()
}

pub(crate) fn inverted_range(c: &Cst, s: &mut Sink) {
    for n in c.nodes(OpenRange) {
        let kids = child_nodes(c, n);
        let [l, r] = kids[..] else { continue };
        if c.kind(l) != Literal || c.kind(r) != Literal {
            continue;
        }
        let (Some(lo), Some(hi)) = (int_literal(&c.trimmed(l)), int_literal(&c.trimmed(r))) else { continue };
        let Some(open) = c.tok(n, "[") else { continue };
        if lo > hi {
            s.add(
                "SV-RND-021",
                "ConstraintInsideInvertedRange",
                Severity::Error,
                format!(
                    "Range expression '[{lo} : {hi}]' inside set membership specifies an inverted range where lower bound exceeds upper bound ({lo} > {hi}). \
                     In SystemVerilog (IEEE 1800 § 18.5.7 and IEEE 1800 § 18.5.4), an inverted range defines an empty set, causing \
                     constraint solver contradictions and runtime randomization failure."
                ),
                c.loc(open),
            );
        }
    }
}
