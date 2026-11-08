//! SV-COV-017 (UnreachableCoverBin): Flags coverpoint bins whose values all lie
//! outside the range of the coverpoint expression (for example bins {[16:31]}
//! on a 4-bit value).

use crate::support::slang_ast::*;
use crate::support::sva_cov::{add_n, bins_kw, bins_name, label_of};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

/// Width and signedness of a declared integral type, as written.
fn type_width(c: &Cst, dt: NodeId) -> Option<(u32, bool)> {
    let toks: Vec<&str> = c.toks(dt).map(|t| c.text_of(t)).collect();
    let base = *toks.first()?;
    let explicit_signed = toks.contains(&"signed");
    let explicit_unsigned = toks.contains(&"unsigned");
    let (w, signed) = match base {
        "bit" | "logic" | "reg" => (1, false),
        "byte" => (8, true),
        "shortint" => (16, true),
        "int" | "integer" => (32, true),
        "longint" => (64, true),
        _ => return None,
    };
    let signed = (signed || explicit_signed) && !explicit_unsigned;
    let dims: Vec<NodeId> = c.tree.children(dt).filter(|&d| c.kind(d) == Dim).collect();
    if dims.is_empty() {
        return Some((w, signed));
    }
    if matches!(base, "byte" | "shortint" | "int" | "integer" | "longint") {
        return None;
    }
    let mut total = 1u32;
    for d in dims {
        let k = kids(c, d);
        if k.len() != 2 {
            return None;
        }
        let a = const_value(c, k[0], 0)?;
        let b = const_value(c, k[1], 0)?;
        total = total.checked_mul(((a - b).unsigned_abs() + 1) as u32)?;
    }
    Some((total, signed))
}

/// The value of a constant integer expression as written: literals,
/// negation, parameters and simple arithmetic.
fn const_value(c: &Cst, e: NodeId, depth: u32) -> Option<i128> {
    if depth > 16 {
        return None;
    }
    match c.kind(e) {
        Literal => literal_value(c, e),
        ParenExpr => const_value(c, *kids(c, e).first()?, depth + 1),
        PrefixExpr => {
            let op = c.keyword(e).map(|t| c.text_of(t))?;
            let v = const_value(c, *kids(c, e).first()?, depth + 1)?;
            match op {
                "-" => Some(-v),
                "+" => Some(v),
                _ => None,
            }
        }
        BinaryExpr => {
            let op = c.keyword(e).map(|t| c.text_of(t))?;
            let k = kids(c, e);
            let a = const_value(c, *k.first()?, depth + 1)?;
            let b = const_value(c, *k.get(1)?, depth + 1)?;
            match op {
                "+" => a.checked_add(b),
                "-" => a.checked_sub(b),
                "*" => a.checked_mul(b),
                _ => None,
            }
        }
        NameExpr => {
            let t = c.first_tok(e)?;
            let d = lookup(c, e, c.text_of(t))?;
            if d.kind != DeclKind::Parameter {
                return None;
            }
            let init = kids(c, d.node).into_iter().filter(|&k| c.kind(k) != Name).last()?;
            const_value(c, init, depth + 1)
        }
        _ => None,
    }
}

fn literal_value(c: &Cst, e: NodeId) -> Option<i128> {
    let text: String = c.tree.token_range(e).into_iter().map(|t| c.text_of(t)).collect::<String>().replace('_', "");
    if let Some(q) = text.find('\'') {
        let rest = &text[q + 1..];
        let rest = rest.strip_prefix(['s', 'S']).unwrap_or(rest);
        let (base, digits) = match rest.chars().next()? {
            'd' | 'D' => (10, &rest[1..]),
            'h' | 'H' => (16, &rest[1..]),
            'b' | 'B' => (2, &rest[1..]),
            'o' | 'O' => (8, &rest[1..]),
            _ => return None,
        };
        if digits.is_empty() || digits.chars().any(|ch| matches!(ch, 'x' | 'X' | 'z' | 'Z' | '?')) {
            return None;
        }
        return i128::from_str_radix(digits, base).ok();
    }
    if text.bytes().all(|b| b.is_ascii_digit()) && !text.is_empty() {
        return text.parse().ok();
    }
    None
}

pub(crate) fn unreachable_bin(c: &Cst, s: &mut Sink) {
    for cp in c.nodes(Coverpoint) {
        let Some(expr) =
            kids(c, cp).into_iter().find(|&k| !matches!(c.kind(k), StmtLabel | Attribute | Bins | CoverageOption))
        else {
            continue;
        };
        let cp_name = match label_of(c, cp) {
            Some(l) => l,
            None if c.kind(expr) == NameExpr => c.tree.bare_text(expr),
            None => String::new(),
        };
        let decl = match c.kind(expr) {
            NameExpr => c.first_tok(expr).and_then(|t| lookup(c, expr, c.text_of(t))),
            MemberExpr => {
                let base = kids(c, expr).first().copied();
                base.and_then(|b| expr_type(c, b))
                    .and_then(|t| find_class(c, &t, expr))
                    .and_then(|k| class_member(c, k, c.last_text(expr)))
            }
            _ => None,
        };
        // As written, else the expression's type (a width set by a
        // parameter of another file or package, `bit [XLEN-1:0]`).
        let typed = || {
            crate::expr_types::expr_types(c).get(&(c.index(), expr)).and_then(|t| t.integral).map(|(w, s, _)| (w, s))
        };
        let Some((width, signed)) = decl.and_then(|d| d.dtype).and_then(|dt| type_width(c, dt)).or_else(typed) else {
            continue;
        };
        if width == 0 || width > 62 {
            continue;
        }
        let span = 1i128 << width;
        let (lo_d, hi_d) = if signed { (-span / 2, span / 2 - 1) } else { (0, span - 1) };
        let n = times(c, cp);
        for b in kids(c, cp) {
            if c.kind(b) != Bins || bins_kw(c, b) != "bins" {
                continue;
            }
            let Some(rl) = c.tree.child(b, RangeList) else { continue };
            let vals = kids(c, rl);
            if vals.is_empty() {
                continue;
            }
            let (mut reachable, mut known) = (false, true);
            for v in vals {
                let (lo, hi) = if c.kind(v) == OpenRange {
                    let k = kids(c, v);
                    if k.len() != 2 {
                        known = false;
                        break;
                    }
                    let bound = |x: NodeId, inf: i128| -> Option<i128> {
                        if c.kind(x) == NameExpr && c.tree.bare_text(x) == "$" {
                            Some(inf)
                        } else {
                            const_value(c, x, 0)
                        }
                    };
                    let (Some(l), Some(h)) = (bound(k[0], i128::MIN / 4), bound(k[1], i128::MAX / 4)) else {
                        known = false;
                        break;
                    };
                    (l.min(h), l.max(h))
                } else {
                    let Some(x) = const_value(c, v, 0) else {
                        known = false;
                        break;
                    };
                    (x, x)
                };
                if hi >= lo_d && lo <= hi_d {
                    reachable = true;
                    break;
                }
            }
            if !known || reachable {
                continue;
            }
            let Some((bname, t)) = bins_name(c, b) else { continue };
            add_n(
                s,
                n,
                "SV-COV-017",
                "UnreachableCoverBin",
                Severity::Warning,
                format!(
                    "Bin '{bname}' of coverpoint '{cp_name}' only lists values outside what the {width}-bit {} coverpoint can hold ({lo_d} to {hi_d}). It can never be hit, so coverage \
                     never reaches 100%. Fix the range or widen the coverpoint expression.",
                    if signed { "signed" } else { "unsigned" }
                ),
                c.loc(t),
            );
        }
    }
}
