//! UVM-STY-003 (ComponentNameMismatch): Disallows passing hardcoded string
//! literals to super.new() in uvm_component constructors.

use crate::support::uvm_classes::{comp_decl, routines_in, Ix};
use crate::support::uvm_syntax::{arg_list, name_node, ordered_expr, params, routine_last_name, subtree};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use svlang::syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn component_name_mismatch(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for c in csts {
        for (cls, f) in routines_in(&ix, c, comp_decl) {
            if routine_last_name(c, f) != "new" {
                continue;
            }
            let mut param = "name".to_string();
            if let Some(pl) = c.tree.child(f, PortList) {
                if let Some(nm) = c
                    .tree
                    .children(pl)
                    .filter(|&p| c.kind(p) == Port)
                    .find_map(|p| c.tree.child(p, Declarator).and_then(|d| name_node(c, d)))
                {
                    param = c.tree.bare_text(nm);
                }
            }
            let mut list = None;
            for n in subtree(c, f) {
                let found = match c.kind(n) {
                    CallExpr => c.tree.children(n).next().is_some_and(|l| c.text(l).contains("super.new")),
                    NewExpr => false,
                    _ => false,
                };
                if found {
                    list = arg_list(c, n);
                }
            }
            let Some(list) = list else { continue };
            let Some(Some(a0)) = params(c, list).first().copied() else { continue };
            let Some(e) = ordered_expr(c, a0) else { continue };
            let text = c.trimmed(e);
            if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
                let Some(t) = c.first_tok(e) else { continue };
                s.add(
                    "UVM-STY-003",
                    "ComponentNameMismatch",
                    Severity::Error,
                    format!(
                        "Constructor of UVM component '{cls}' passes hardcoded string literal {text} to super.new(...) instead of forwarding parameter '{param}'. Hardcoded component names override factory instance names and break hierarchical config_db resolution."
                    ),
                    c.loc(t),
                )
                .fixes
                .extend((!param.is_empty()).then(|| unsafe_fix(c.tree.token_bounds(e).and_then(|(a, b)| {
                    c.token_fix(a, b, param.clone(), &format!("Replace hardcoded literal with constructor parameter '{param}'"))
                }))).flatten());
            }
        }
    }
}

fn unsafe_fix(f: Option<crate::fix::Fix>) -> Option<crate::fix::Fix> {
    let mut x = f?;
    x.safety = crate::fix::FixSafety::Unsafe;
    Some(x)
}
