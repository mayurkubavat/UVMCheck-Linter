//! UVM-RUS-005 (OverrideByNameString): Flags factory overrides and creates that
//! name types with string literals (set_type_override_by_name,
//! set_inst_override, create_object ...).

use crate::support::class_roles::Index;
use crate::support::free_routines::free_routines;
use crate::support::routine_bodies::{bodies, calls_in, Calls};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use std::collections::HashMap;
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

/// Factory calls that take type names as strings, and which arguments
/// those are.
fn type_name_args(owner: &str, n: &str) -> &'static [usize] {
    match (owner, n) {
        ("uvm_factory", "set_type_override_by_name" | "set_inst_override_by_name") => &[0, 1],
        ("uvm_factory", "create_object_by_name" | "create_component_by_name") => &[0],
        ("uvm_component", "set_type_override") => &[0, 1],
        ("uvm_component", "set_inst_override") => &[1, 2],
        ("uvm_component", "create_object" | "create_component") => &[0],
        _ => &[],
    }
}

fn report_override_by_name(c: &Cst, call: NodeId, owner: &str, out: &mut Sink) {
    let n = c.callee_name(call);
    let idx = type_name_args(owner, n);
    if idx.is_empty() {
        return;
    }
    let args = c.call_args(call);
    let mut names = Vec::new();
    for &i in idx {
        let Some(v) = args.get(i).and_then(|a| c.arg_expr(*a)).and_then(|e| c.string_value(e)) else { return };
        names.push(format!("\"{v}\""));
    }
    out.add(
        "UVM-RUS-005",
        "OverrideByNameString",
        Severity::Warning,
        format!(
            "{n}() names types as strings ({}). A misspelt or renamed type is only noticed when the test runs, and often \
not even then. Use the typed form, such as base_t::type_id::set_type_override(derived_t::get_type()).",
            names.join(", ")
        ),
        c.loc_of(call),
    );
}

/// Declared types of the names a free routine declares (ports and locals).
fn routine_names(c: &Cst, r: NodeId) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for n in c.subtree(r) {
        if !matches!(c.kind(n), Port | DataDecl) {
            continue;
        }
        let Some(ty) = c.tree.child(n, DataType) else { continue };
        for dcl in c.tree.children(n).filter(|&x| c.kind(x) == Declarator) {
            if let Some(name) = c.tree.child(dcl, Name) {
                out.entry(c.tree.bare_text(name)).or_insert_with(|| c.tree.bare_text(ty));
            }
        }
    }
    out
}

pub(crate) fn override_by_name_string(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for (_, m, c, body) in bodies(d, &ix) {
        let calls = Calls::new(m);
        for call in calls_in(c, body) {
            report_override_by_name(c, call, calls.owner(c, call), out);
        }
    }
    for c in csts {
        for r in free_routines(d, c) {
            let names = routine_names(c, r);
            let Some(body) = c.tree.child(r, Body) else { continue };
            for call in calls_in(c, body) {
                let Some(callee) = c.tree.children(call).next() else { continue };
                if c.kind(callee) != MemberExpr {
                    continue;
                }
                let Some(obj) = c.tree.children(callee).next() else { continue };
                if c.kind(obj) != NameExpr {
                    continue;
                }
                let Some(ty) = names.get(&c.tree.bare_text(obj)) else { continue };
                let Some(k) = crate::support::class_roles::class_of_type_in(d, Some(&ix.typedefs), ty, None) else {
                    continue;
                };
                let n = c.callee_name(call);
                let owner = std::iter::once(k)
                    .chain(d.ancestors(k.id))
                    .find(|x| x.methods.iter().any(|mm| mm.name == n))
                    .map(|x| x.name.as_str())
                    .unwrap_or("");
                report_override_by_name(c, call, owner, out);
            }
        }
    }
}
