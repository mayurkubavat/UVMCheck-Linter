//! SV-RDB-006 (DoubleNegativeFlag): Flags '!' (or '~' on one bit) applied to a
//! variable whose name is negative (disable_*, no_*, not_*, skip_*, inhibit_*,
//! ...).

use crate::expr_types::expr_types;
use crate::support::config_db_calls::{strip, user_visits, with_parens};
use crate::support::slang_ast::kids;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

/// The words of a name: split at `_`, digits and lower-to-upper case steps,
/// lowercased.
fn ident_words(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in name.chars() {
        if ch == '_' || ch.is_ascii_digit() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur).to_lowercase());
            }
            continue;
        }
        if ch.is_ascii_uppercase() && cur.chars().last().is_some_and(|l| l.is_ascii_lowercase()) {
            out.push(std::mem::take(&mut cur).to_lowercase());
        }
        cur.push(ch);
    }
    if !cur.is_empty() {
        out.push(cur.to_lowercase());
    }
    out
}

fn negative_name(name: &str) -> bool {
    const NEG: &[&str] =
        &["disable", "disabled", "dis", "no", "not", "inhibit", "skip", "ignore", "without", "non", "suppress"];
    ident_words(name).iter().any(|w| NEG.contains(&w.as_str()))
}

/// A variable (not a net, argument or parameter) or a class property.
fn user_variable(c: &Cst, d: &crate::support::slang_ast::Decl) -> bool {
    use crate::support::slang_ast::DeclKind::*;
    match d.kind {
        Property | Local => true,
        Static => {
            const NETS: &[&str] = &[
                "wire", "tri", "wand", "wor", "triand", "trior", "tri0", "tri1", "supply0", "supply1", "uwire",
                "trireg",
            ];
            // The declaration holding the declarator.
            let Some(holder) = c.parent(d.node) else { return false };
            if c.toks(holder).any(|t| NETS.contains(&c.text_of(t)))
                || d.dtype.is_some_and(|t| c.toks(t).any(|t| NETS.contains(&c.text_of(t))))
            {
                return false;
            }
            // A port without a data type is a net.
            if c.kind(holder) == Port {
                // An input or inout port is a net unless declared `var`.
                let toks: Vec<&str> = c.toks(holder).map(|t| c.text_of(t)).collect();
                if toks.iter().any(|t| matches!(*t, "input" | "inout")) && !toks.contains(&"var") {
                    return false;
                }
                return d
                    .dtype
                    .and_then(|t| c.first_tok(t))
                    .is_some_and(|f| !matches!(c.text_of(f), "signed" | "unsigned" | "["));
            }
            true
        }
        _ => false,
    }
}

pub(crate) fn double_negative_flag(c: &Cst, out: &mut Sink) {
    let types = expr_types(c);
    let ix = c.index();
    for e in c.nodes(PrefixExpr) {
        let Some(op) = c.keyword(e).map(|k| c.text_of(k)) else { continue };
        if op != "!" && op != "~" {
            continue;
        }
        let Some(&operand) = kids(c, e).first() else { continue };
        let o = strip(c, operand);
        if op == "~" && !types.get(&(ix, o)).and_then(|t| t.integral).is_some_and(|(w, ..)| w == 1) {
            continue;
        }
        if !matches!(c.kind(o), NameExpr | MemberExpr) {
            continue;
        }
        let Some(d) = crate::support::handles::expr_decl(c, o) else { continue };
        let name = if c.kind(o) == MemberExpr { c.last_text(o).to_string() } else { c.tree.bare_text(o) };
        if !user_variable(c, &d) || !negative_name(&name) {
            continue;
        }
        let times = user_visits(c, e);
        if times == 0 || crate::support::statements::in_bad_body(c, e) {
            continue;
        }
        let Some(t) = c.first_tok(e) else { continue };
        let msg = format!(
            "'{}' negates a negative flag ('{name}'), which readers have to invert twice. Name the flag for the positive case (en_*, has_*, use_*) so the test reads straight.",
            crate::support::type_text::expr_text(c, with_parens(c, e))
        );
        for _ in 0..times {
            out.add("SV-RDB-006", "DoubleNegativeFlag", Severity::Info, msg.clone(), c.loc(t));
        }
    }
}
