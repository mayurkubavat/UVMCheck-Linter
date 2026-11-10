//! SV-COV-016 (OverlappingRangesInCoverpointBin): Flags coverpoint bins
//! containing overlapping numerical value ranges.

use crate::support::slang_ast::*;
use crate::support::sva_cov::bins_name;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

/// `std::stoll(text, nullptr, 0)` of a decimal literal as written.
fn stoll0(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.is_empty() || !b[0].is_ascii_digit() {
        return None;
    }
    let (base, digits) = if b[0] == b'0' && b.len() > 1 && (b[1] == b'x' || b[1] == b'X') {
        (16, &text[2..])
    } else if b[0] == b'0' {
        (8, text)
    } else {
        (10, text)
    };
    let mut v: i64 = 0;
    let mut any = false;
    for ch in digits.chars() {
        let Some(d) = ch.to_digit(base) else { break };
        v = v.checked_mul(base as i64)?.checked_add(d as i64)?;
        any = true;
    }
    any.then_some(v)
}

/// slang's `IntegerLiteralExpression`: an unsized decimal number.
fn int_literal(c: &Cst, e: NodeId) -> Option<i64> {
    if c.kind(e) != Literal {
        return None;
    }
    let toks: Vec<u32> = c.toks(e).collect();
    if toks.len() != 1 {
        return None;
    }
    let t = c.text_of(toks[0]);
    if !t.bytes().all(|ch| ch.is_ascii_digit() || ch == b'_') {
        return None;
    }
    stoll0(t)
}

pub(crate) fn overlapping_ranges(c: &Cst, s: &mut Sink) {
    for b in c.nodes(Bins) {
        let mut ranges = Vec::new();
        for r in subtree(c, b) {
            if c.kind(r) != OpenRange {
                continue;
            }
            let k = kids(c, r);
            if k.len() != 2 {
                continue;
            }
            if let (Some(lo), Some(hi)) = (int_literal(c, k[0]), int_literal(c, k[1])) {
                if lo <= hi {
                    ranges.push((lo, hi));
                }
            }
        }
        'found: for i in 0..ranges.len() {
            for j in i + 1..ranges.len() {
                let (a, b2) = (ranges[i], ranges[j]);
                if a.0.max(b2.0) <= a.1.min(b2.1) {
                    let Some((name, t)) = bins_name(c, b) else { break 'found };
                    s.add(
                        "SV-COV-016",
                        "OverlappingRangesInCoverpointBin",
                        Severity::Warning,
                        format!(
                            "Coverpoint bin '{name}' contains overlapping value ranges '[{}:{}]' and '[{}:{}]'. In functional coverage \
                             methodology (IEEE 1800 § 19.5), overlapping sub-ranges within a single \
                             bin definition are redundant and indicate specification errors in the functional coverage model.",
                            a.0, a.1, b2.0, b2.1
                        ),
                        c.loc(t),
                    );
                    break 'found;
                }
            }
        }
    }
}
