//! SV-TYP-001 (MixedSignComparison): Comparing signed and unsigned integers
//! causes implicit unsigned coercion of the signed operand.

use crate::expr_types::{visit_types, ETy};
use crate::support::config_db_calls::{in_generate_header, strip};
use crate::support::slang_ast::kids;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

/// An integer literal (not `'0`) whose value is not negative.
fn non_negative_literal(c: &Cst, e: NodeId, t: &ETy) -> bool {
    if crate::expr_types::slang_kind(c, e) != Some("IntegerLiteral") {
        return false;
    }
    let text = c.tree.bare_text(e).replace('_', "");
    // A signed based literal with its top bit set is negative.
    let Some(q) = text.find('\'') else { return true };
    let rest = &text[q + 1..];
    if !rest.starts_with(['s', 'S']) {
        return true;
    }
    let (w, ..) = t.integral.unwrap_or((32, true, false));
    let digits = &rest[2..];
    let bits: String = match rest.as_bytes().get(1).map(|b| b.to_ascii_lowercase()) {
        Some(b'b') => digits.to_string(),
        Some(b'h') => digits.chars().map(|d| d.to_digit(16).map_or("xxxx".into(), |v| format!("{v:04b}"))).collect(),
        Some(b'o') => digits.chars().map(|d| d.to_digit(8).map_or("xxx".into(), |v| format!("{v:03b}"))).collect(),
        Some(b'd') => return digits.parse::<u128>().map_or(true, |v| w >= 128 || v >> (w - 1) == 0),
        _ => return true,
    };
    let bits = bits.trim_start_matches('0');
    (bits.len() as u32) < w || !bits.starts_with('1')
}

const RELATIONAL: &[&str] = &["<", "<=", ">", ">=", "==", "!=", "===", "!=="];

/// A comparison of a signed and an unsigned operand, unless the signed one
/// is a non-negative literal: once per AST visit.
pub(crate) fn mixed_sign_comparison(c: &Cst, out: &mut Sink) {
    for e in c.nodes(BinaryExpr) {
        let op = c.keyword(e).map_or("", |k| c.text_of(k));
        if !RELATIONAL.contains(&op) || (op == "<=" && c.parent(e).is_some_and(|p| c.kind(p) == ExprStmt)) {
            continue;
        }
        let ks = kids(c, e);
        let (Some(&l), Some(&r)) = (ks.first(), ks.get(1)) else { continue };
        let (l, r) = (strip(c, l), strip(c, r));
        if in_generate_header(c, e) {
            continue;
        }
        let Some(t) = c.first_tok(e) else { continue };
        // Per instance where a module's instances type them differently.
        for (k, ts) in visit_types(c, e, &[l, r]) {
            let (Some((lt, _)), Some((rt, _))) = (&ts[0], &ts[1]) else { continue };
            let (Some((_, ls, _)), Some((_, rs, _))) = (lt.integral, rt.integral) else { continue };
            if ls == rs {
                continue;
            }
            let flag = (ls && !non_negative_literal(c, l, lt)) || (rs && !non_negative_literal(c, r, rt));
            if !flag {
                continue;
            }
            let side = if ls { "LHS" } else { "RHS" };
            for _ in 0..k {
                out.add(
                    "SV-TYP-001",
                    "MixedSignComparison",
                    Severity::Warning,
                    format!(
                        "Comparison between signed type '{}' and unsigned type '{}'. In SystemVerilog (IEEE 1800 Sec 11.8.2), if either operand is unsigned, the entire comparison is coerced to unsigned, causing negative numbers on the {side} to evaluate as large positive numbers.",
                        lt.text, rt.text
                    ),
                    c.loc(t),
                );
            }
        }
    }
}
