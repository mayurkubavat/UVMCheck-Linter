//! SVA-VAC-001 (SvaVacuousAntecedent): Detects constant-false antecedents in
//! assertion implications (0 |-> ...).
//!
//! Constant false: a false literal, or `&&` terms of which one is the
//! negation of another (`req && !req`). Other `a && !b` antecedents
//! (`valid && !ready`) are ordinary conditions. The C++ rule is the same.

/// `e` without parentheses around the whole of it.
fn unparen(e: &str) -> &str {
    let mut e = e.trim();
    while e.starts_with('(') && e.ends_with(')') {
        let mut depth = 0;
        let whole = e.char_indices().all(|(i, ch)| {
            match ch {
                '(' => depth += 1,
                ')' => depth -= 1,
                _ => {}
            }
            depth > 0 || i == e.len() - 1
        });
        if !whole {
            break;
        }
        e = e[1..e.len() - 1].trim();
    }
    e
}

/// The `&&` terms of `e` at its top level.
fn conjuncts(e: &str) -> Vec<&str> {
    let (mut out, mut depth, mut start) = (Vec::new(), 0i32, 0);
    let b = e.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'&' if depth == 0 && b.get(i + 1) == Some(&b'&') => {
                out.push(&e[start..i]);
                start = i + 2;
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    out.push(&e[start..]);
    out
}

/// One `&&` term is the logical or bitwise negation of another.
pub(crate) fn contradictory(ant: &str) -> bool {
    let norm = |x: &str| unparen(x).chars().filter(|ch| !ch.is_whitespace()).collect::<String>();
    let (mut pos, mut neg) = (Vec::new(), Vec::new());
    for t in conjuncts(unparen(ant)) {
        let t = unparen(t);
        let negated = (t.starts_with('!') && !t.starts_with("!="))
            || (t.starts_with('~') && !t[1..].starts_with(['&', '|', '^']));
        if negated {
            neg.push(norm(&t[1..]));
        } else {
            pos.push(norm(t));
        }
    }
    pos.iter().any(|p| neg.contains(p))
}

use crate::support::cross_labels::kids;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind::*;

pub(crate) fn vacuous_antecedent(c: &Cst, s: &mut Sink) {
    for n in c.nodes(SequenceExpr) {
        if c.tok(n, "|->").is_none() && c.tok(n, "|=>").is_none() {
            continue;
        }
        let Some(&left) = kids(c, n).first() else { continue };
        let ant = c.trimmed(left);
        let vacuous = matches!(ant.as_str(), "0" | "1'b0" | "'0" | "false" | "1'sb0") || contradictory(&ant);
        let Some(t) = c.first_tok(left).filter(|_| vacuous) else { continue };
        s.add(
            "SVA-VAC-001",
            "SvaVacuousAntecedent",
            Severity::Error,
            format!(
                "Vacuous assertion implication detected: antecedent '{ant}' evaluates to constant false. \
                 In SVA (IEEE 1800 § 16.12.22), a false antecedent makes the implication evaluate to true on every cycle, \
                 causing the assertion to pass vacuously 100% of the time and masking unverified DUT logic."
            ),
            c.loc(t),
        );
    }
}
