//! UVM-CFG-001 (UvmConfigDbTypeMismatch): The type parameter in
//! uvm_config_db#(T)::get must match the type parameter used in the
//! corresponding set call for the same field name.
//!
//! A type parameter on either side (`dv_base_agent::CFG_T`, or the `T` of
//! `uvm_config_db #(T)` inside a generic wrapper) is whatever the
//! specialization makes it, so the pair is not compared. The C++ rule skips
//! the same pairs.

use crate::support::config_db_calls::{config_db_calls, CfgCall};
use crate::syntax::Cst;
use crate::{Severity, Sink};

/// Type parameters by class name: `class c #(type T = int)` -> c -> {T}.
fn type_params(csts: &[Cst]) -> std::collections::HashMap<String, Vec<String>> {
    use sv_syntax::SyntaxKind::*;
    let mut out: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for c in csts {
        for k in c.nodes(ClassDecl) {
            let Some(pl) = c.tree.child(k, Header).and_then(|h| c.tree.child(h, ParamList)) else { continue };
            let mut typed = false;
            for p in c.tree.children(pl).filter(|&p| c.kind(p) == ParamDecl) {
                // `#(type A = int, B = A)`: B is a type parameter too.
                typed = c.toks(p).any(|t| c.text_of(t) == "type") || (typed && c.toks(p).next().is_none());
                if !typed {
                    continue;
                }
                let names = c.tree.children(p).filter_map(|x| match c.kind(x) {
                    Name => Some(c.tree.bare_text(x)),
                    Declarator => c.tree.child(x, Name).map(|n| c.tree.bare_text(n)),
                    _ => None,
                });
                out.entry(crate::support::slang_ast::decl_name(c, k)).or_default().extend(names);
            }
        }
    }
    out
}

pub(crate) fn config_db_type_mismatch(_: &dv_model::Design, csts: &[Cst], out: &mut Sink) {
    let calls = config_db_calls(csts);
    let params = type_params(csts);
    // `pkg::cls::T` where T is a type parameter of cls.
    let is_param = |ty: &str| {
        let parts: Vec<&str> = ty.split("::").collect();
        parts.len() >= 2
            && params.get(parts[parts.len() - 2]).is_some_and(|ps| ps.iter().any(|p| p == parts[parts.len() - 1]))
    };
    // The set the AST walk meets last for each field.
    let mut sets: std::collections::HashMap<&str, &CfgCall> = std::collections::HashMap::new();
    for k in calls.iter().filter(|k| k.set && k.visits > 0) {
        sets.insert(&k.field, k);
    }
    for g in calls.iter().filter(|k| !k.set) {
        let Some(s) = sets.get(g.field.as_str()) else { continue };
        if s.ty == g.ty || is_param(&s.ty) || is_param(&g.ty) {
            continue;
        }
        for _ in 0..g.visits {
            out.add(
                "UVM-CFG-001",
                "UvmConfigDbTypeMismatch",
                Severity::Error,
                format!(
                    "Type mismatch in 'uvm_config_db' for field '{}': value was configured as type '{}', but is retrieved as type '{}'. Mismatched types prevent configuration retrieval at runtime.",
                    g.field, s.ty, g.ty
                ),
                g.loc,
            );
        }
    }
}
