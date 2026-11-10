//! UVM-STY-001 (UvmCreateStringMismatch): String argument passed to
//! type_id::create() must match the receiving variable name.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind::*;

pub(crate) fn create_string_mismatch(c: &Cst, s: &mut Sink) {
    for n in c.nodes(BinaryExpr) {
        let Some(op) = c.toks(n).next() else { continue };
        if c.text_of(op) != "=" {
            continue;
        }
        let mut sides = c.tree.children(n);
        let (Some(l), Some(r)) = (sides.next(), sides.next()) else { continue };
        let var = match c.kind(l) {
            NameExpr if c.tree.token_bounds(l).is_some_and(|(a, b)| a == b) => c.last_text(l).to_string(),
            MemberExpr => c.last_text(l).to_string(),
            _ => String::new(),
        };
        if var.is_empty() || c.kind(r) != CallExpr {
            continue;
        }
        let Some(callee) = c.tree.children(r).next() else { continue };
        let callee_text: String = c.text(callee).chars().filter(|ch| !ch.is_whitespace()).collect();
        if !(callee_text.ends_with("::create") || callee_text.ends_with(".create") || callee_text == "create") {
            continue;
        }
        let Some(args) = c.tree.child(r, ArgList) else { continue };
        let Some(arg0) = c.items(args).into_iter().find(|&x| c.kind(x) == Arg) else { continue };
        let Some(expr) = c.items(arg0).into_iter().next() else { continue };
        let raw = c.trimmed(expr);
        if raw.len() < 2 || !raw.starts_with('"') || !raw.ends_with('"') {
            continue;
        }
        let val = &raw[1..raw.len() - 1];
        let ok = val == var || var.strip_prefix("m_") == Some(val) || val.strip_prefix("m_") == Some(var.as_str());
        if ok {
            continue;
        }
        let Some(first) = c.first_tok(expr) else { continue };
        s.add(
            "UVM-STY-001",
            "UvmCreateStringMismatch",
            Severity::Warning,
            format!(
                "UVM factory create instance string '\"{val}\"' does not match target variable identifier '{var}'. In UVM guidelines, instance names must match the variable name to ensure predictable uvm_config_db scoping."
            ),
            c.loc(first),
        );
    }
}
