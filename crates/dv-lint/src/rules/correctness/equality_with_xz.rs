//! SV-LANG-001 (EqualityWithXz): Detects equality ('==', '!=') checks against
//! 4-state unknown literals ('x, 'z).

use crate::support::class_fields::declared_type;
use crate::support::class_index::Classes;
use crate::support::literals::{ast_times, integer_literal, unparen};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::{NodeId, SyntaxKind, TokenKind};
use SyntaxKind::*;

fn has_unknown(digits: &str) -> bool {
    digits.chars().any(|ch| matches!(ch, 'x' | 'X' | 'z' | 'Z' | '?'))
}

/// Bits of a simple integral type (`logic [3:0]`, `int`), and whether it
/// is four-state.
fn type_bits(c: &Cst, dt: NodeId) -> Option<(u64, bool)> {
    let text: String = c.text(dt).chars().filter(|ch| !ch.is_whitespace()).collect();
    let (kw, rest) = match text.find('[') {
        Some(i) => (&text[..i], &text[i..]),
        None => (text.as_str(), ""),
    };
    let kw = kw.trim_end_matches("signed").trim_end_matches("un");
    let (mut w, four): (u64, bool) = match kw {
        "logic" | "reg" | "" => (1, true),
        "bit" => (1, false),
        "byte" => (8, false),
        "shortint" => (16, false),
        "int" => (32, false),
        "integer" => (32, true),
        "longint" => (64, false),
        _ => return None,
    };
    let mut r = rest;
    while let Some(body) = r.strip_prefix('[') {
        let end = body.find(']')?;
        let (a, b) = body[..end].split_once(':')?;
        let (a, b): (i64, i64) = (a.parse().ok()?, b.parse().ok()?);
        w *= a.abs_diff(b) + 1;
        r = &body[end + 1..];
    }
    r.is_empty().then_some((w, four))
}

/// The literal operand with x or z bits, and whether slang leaves it
/// unconverted: unbased unsized literals take the other side's type; a
/// sized one stays as it is when it is at least as wide as the other
/// side (a simple variable; other operands are taken to match).
fn unknown_operand(c: &Cst, classes: &Classes, e: NodeId, other: NodeId) -> bool {
    let e = unparen(c, e);
    if c.kind(e) != Literal {
        return false;
    }
    let toks = c.tree.token_range(e);
    if toks.len() == 1 && c.tree.tokens[toks[0] as usize].kind == TokenKind::UnbasedUnsizedLiteral {
        let t = c.text_of(toks[0]);
        return matches!(t, "'x" | "'X" | "'z" | "'Z");
    }
    let Some(lit) = integer_literal(c, e) else { return false };
    if !lit.based || !has_unknown(&lit.digits) {
        return false;
    }
    let width: u64 = if lit.sized {
        c.text_of(toks[0]).chars().filter(|&ch| ch != '_').collect::<String>().parse().unwrap_or(32)
    } else {
        32
    };
    let other = unparen(c, other);
    if c.kind(other) == NameExpr {
        let name = c.tree.bare_text(other);
        if let Some((w, _)) = declared_type(c, classes, other, &name).and_then(|dt| type_bits(c, dt)) {
            return width >= w;
        }
    }
    true
}

pub(crate) fn equality_with_xz(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    for n in c.nodes(BinaryExpr) {
        let op = c.tok(n, "==").or_else(|| c.tok(n, "!="));
        let Some(op) = op else { continue };
        let kids: Vec<NodeId> = c.tree.children(n).collect();
        let [l, r] = kids[..] else { continue };
        if !(unknown_operand(c, &classes, l, r) || unknown_operand(c, &classes, r, l)) {
            continue;
        }
        let times = ast_times(c, n);
        let (o, case) = if c.text_of(op) == "==" { ("==", "===") } else { ("!=", "!==") };
        let Some(at) = c.first_tok(n) else { continue };
        let fix = crate::fix::with_safety(
            c.token_fix(op, op, case, &format!("Replace '{o}' with case equality '{case}'")),
            crate::fix::FixSafety::Unsafe,
        );
        for _ in 0..times {
            s.add(
                "SV-LANG-001",
                "EqualityWithXz",
                Severity::Error,
                format!(
                    "Dangerous 4-state equality check '{o}' against unknown literal (x/z). In SystemVerilog (IEEE 1800 § 11.4.5), logical equality ('==', '!=') against 'x' or 'z' evaluates to 'x' (which evaluates to false in conditionals), silently masking verification failures. Use case equality ('{case}') or wildcard equality ('==?', '!=?')."
                ),
                c.loc(at),
            )
            .fixes
            .extend(fix.clone());
        }
    }
}
