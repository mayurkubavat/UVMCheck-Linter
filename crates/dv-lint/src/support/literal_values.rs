//! Literal and integer values of expressions, and the declaration an
//! expression names.

use sv_syntax::{NodeId, SyntaxKind};

use crate::syntax::Cst;

use SyntaxKind::*;

/// The declaration a name or member expression refers to.
pub(crate) fn decl_of_expr(c: &Cst, e: NodeId) -> Option<(crate::support::slang_ast::Decl, String)> {
    use crate::support::slang_ast::{class_member, expr_type, find_class, lookup};
    match c.kind(e) {
        NameExpr if c.tree.children(e).next().is_none() => {
            let name = c.tree.bare_text(e);
            lookup(c, e, &name).map(|d| (d, name))
        }
        MemberExpr => {
            let base = c.tree.children(e).next()?;
            let member = c.last_text(e).to_string();
            let cls = find_class(c, &expr_type(c, base)?, e)?;
            class_member(c, cls, &member).map(|d| (d, member))
        }
        _ => None,
    }
}

pub(crate) fn int_value(c: &Cst, n: NodeId) -> Option<i64> {
    let n = c.strip(n);
    if c.kind(n) != Literal {
        return None;
    }
    let toks: Vec<u32> = c.toks(n).collect();
    let [t] = toks[..] else { return None };
    let text = c.text_of(t).replace('_', "");
    text.bytes().all(|b| b.is_ascii_digit()).then(|| text.parse().ok()).flatten()
}

/// An integer literal's value, when it has no x/z bits and fits.
pub(crate) fn literal_value(c: &Cst, n: NodeId) -> Option<i64> {
    let toks: Vec<u32> = c.toks(n).collect();
    let texts: Vec<&str> = toks.iter().map(|&t| c.text_of(t)).collect();
    let (base, digits) = match texts[..] {
        [d] if d.bytes().all(|b| b.is_ascii_digit() || b == b'_') => (10, d),
        [b, d] | [_, b, d] if b.starts_with('\'') => {
            let radix = match b.trim_start_matches('\'').trim_start_matches(['s', 'S']).chars().next()? {
                'd' | 'D' => 10,
                'h' | 'H' => 16,
                'o' | 'O' => 8,
                'b' | 'B' => 2,
                _ => return None,
            };
            (radix, d)
        }
        _ => return None,
    };
    let d: String = digits.chars().filter(|&ch| ch != '_').collect();
    i64::from_str_radix(&d, base).ok()
}
