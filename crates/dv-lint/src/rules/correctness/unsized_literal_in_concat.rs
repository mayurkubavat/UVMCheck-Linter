//! SV-LANG-003 (UnsizedLiteralInConcat): Disallow unbased unsized integer
//! literals inside concatenations ({1, 0, data}).

use crate::support::literals::{ast_times, compact, integer_literal, is_streaming, unparen};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

/// A concatenation whose operands slang keeps as written: not the `n{...}`
/// of a replicated assignment pattern, nor an unpacked array
/// concatenation whose element type converts them.
fn packed_concat(c: &Cst, n: NodeId) -> bool {
    let Some(p) = c.parent(n) else { return true };
    if c.kind(p) == AssignPattern {
        return false;
    }
    // An element type other than int converts each literal.
    let unpacked = |d: NodeId| {
        c.kind(d) == Declarator
            && c.tree.child(d, Dim).is_some()
            && c.parent(d).and_then(|dd| c.tree.child(dd, DataType)).is_none_or(|t| compact(c, t) != "int")
    };
    if unpacked(p) {
        return false;
    }
    // `arr = {...}` with `arr` declared with unpacked dimensions.
    if c.kind(p) == BinaryExpr && c.tok(p, "=").is_some() && c.tree.children(p).nth(1) == Some(n) {
        if let Some(lhs) = c.tree.children(p).next().filter(|&l| c.kind(l) == NameExpr) {
            let name = c.tree.bare_text(lhs);
            let decl = c.first_named(Declarator, &name);
            if decl.is_some_and(unpacked) {
                return false;
            }
        }
    }
    true
}

pub(crate) fn unsized_in_concat(c: &Cst, s: &mut Sink) {
    for n in c.nodes(ConcatExpr) {
        if is_streaming(c, n) || !packed_concat(c, n) {
            continue;
        }
        let kids: Vec<NodeId> = c.tree.children(n).collect();
        // `{n{...}}` is a replication: its count is not an operand.
        let replication = c.tok(n, ",").is_none() && kids.len() == 2 && c.kind(kids[1]) == ConcatExpr;
        if replication {
            continue;
        }
        for &k in &kids {
            let e = unparen(c, k);
            if c.kind(e) != Literal {
                continue;
            }
            let Some(lit) = integer_literal(c, e) else { continue };
            if lit.sized || lit.based {
                continue;
            }
            let times = ast_times(c, e);
            if times == 0 {
                continue;
            }
            // slang prints the operand's syntax (parentheses and leading
            // trivia included).
            let text = c.text(k);
            let digits: String = lit.digits.chars().filter(|&ch| ch != '_').collect();
            let value = digits.trim_start_matches('0');
            let replacement = match value {
                "" => "1'b0".to_string(),
                "1" => "1'b1".to_string(),
                _ => format!("32'd{text}"),
            };
            let Some(at) = c.first_tok(e) else { continue };
            let fix = crate::fix::with_safety(
                c.tree.token_bounds(k).and_then(|(a, b)| {
                    c.token_fix(
                        a,
                        b,
                        replacement.clone(),
                        &format!("Replace unsized literal with explicitly sized '{replacement}'"),
                    )
                }),
                crate::fix::FixSafety::Unsafe,
            );
            for _ in 0..times {
                s.add(
                    "SV-LANG-003",
                    "UnsizedLiteralInConcat",
                    Severity::Warning,
                    format!(
                        "Unsized literal '{text}' inside concatenation expands to 32 bits per IEEE 1800 § 11.4.12. If a single bit was intended, specify explicit width (e.g. '{replacement}') to prevent unintended 32-bit vector inflation and truncation."
                    ),
                    c.loc(at),
                )
                .fixes
                .extend(fix.clone());
            }
        }
    }
}
