//! SV-COV-001 (UnreferencedCovergroup): Detects covergroups declared in classes
//! or modules that are never instantiated with 'new()' or never sampled.

use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::HashSet;
use sv_syntax::SyntaxKind::*;

pub(crate) fn unreferenced_covergroup(c: &Cst, s: &mut Sink) {
    // Declared: covergroups of the model's classes.
    let mut declared = Vec::new();
    for cls in model_classes(c) {
        for g in kids(c, cls) {
            if c.kind(g) != CovergroupDecl {
                continue;
            }
            let Some(nm) = c.tree.child(g, Name) else { continue };
            let Some(t) = c.first_tok(nm) else { continue };
            let has_event = c.tree.child(g, TimingControl).is_some() || c.tree.child(g, BlockEvent).is_some();
            declared.push((c.text_of(t).to_string(), decl_name(c, cls), c.loc(t), has_event));
        }
    }
    if declared.is_empty() {
        return;
    }
    let covergroups: HashSet<String> = c.nodes(CovergroupDecl).into_iter().map(|g| decl_name(c, g)).collect();
    let mut inst: HashSet<String> = HashSet::new();
    let mut sampled: HashSet<String> = HashSet::new();
    let strip = |t: String| -> String { t.chars().filter(|ch| !ch.is_whitespace()).collect() };
    for n in c.nodes(NewExpr) {
        let Some(p) = c.parent(n) else { continue };
        // The covergroup type the `new` makes.
        let (ty, lhs) = match c.kind(p) {
            BinaryExpr if c.keyword(p).is_some_and(|t| c.text_of(t) == "=") && kids(c, p).get(1) == Some(&n) => {
                let l = kids(c, p)[0];
                let text = strip(slang_text(c, l));
                let name = text.strip_prefix("this.").unwrap_or(&text).to_string();
                let ty = if covergroups.contains(&name) && !name.contains('.') {
                    Some(name.clone())
                } else {
                    expr_type(c, l).filter(|t| covergroups.contains(t))
                };
                (ty, Some(text))
            }
            Declarator => {
                let ty = c
                    .parent(p)
                    .and_then(|d| c.tree.child(d, DataType))
                    .and_then(|t| c.first_tok(t))
                    .map(|t| c.text_of(t).to_string());
                (ty.filter(|t| covergroups.contains(t)), None)
            }
            _ => (None, None),
        };
        let Some(ty) = ty else { continue };
        if let Some(text) = lhs {
            if let Some(x) = text.strip_prefix("this.") {
                inst.insert(x.to_string());
            }
            inst.insert(text);
        }
        inst.insert(ty);
    }
    for call in c.nodes(CallExpr) {
        let Some((callee, name, _)) = call_parts(c, call) else { continue };
        if name != "sample" {
            continue;
        }
        let text = slang_text(c, callee);
        if let Some(dot) = text.rfind('.') {
            let obj = strip(text[..dot].to_string());
            if let Some(x) = obj.strip_prefix("this.") {
                sampled.insert(x.to_string());
            }
            sampled.insert(obj);
        }
    }
    for (name, cls, loc, has_event) in declared {
        if !inst.contains(&name) {
            s.add(
                "SV-COV-001",
                "UnreferencedCovergroup",
                Severity::Warning,
                format!(
                    "Covergroup '{name}' is declared in class '{cls}' but is never instantiated with 'new()'. Uninstantiated covergroups collect 0 coverage data, \
                     masking unverified functional requirements."
                ),
                loc,
            );
        } else if !has_event && !sampled.contains(&name) {
            s.add(
                "SV-COV-001",
                "UnreferencedCovergroup",
                Severity::Warning,
                format!(
                    "Covergroup '{name}' in class '{cls}' is instantiated but has no clocking event and is never sampled via '{name}.sample()'. It will remain at 0% coverage throughout simulation."
                ),
                loc,
            );
        }
    }
}
