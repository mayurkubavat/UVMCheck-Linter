//! SV-DED-014 (UnusedLocalMethod): Flags a local task or function that no
//! method of its class calls.

use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::handles::{body_refs_if_good, method_decl_in, routine_is_virtual};

pub(crate) fn unused_local_method(c: &Cst, out: &mut Sink) {
    use crate::support::slang_ast::{class_methods, decl_name, enclosing_class, has_kw, last_name, user_classes};
    for cls in user_classes(c) {
        let methods = class_methods(c, cls);
        let cname = decl_name(c, cls);
        for &t in &methods {
            let name = last_name(c, t);
            let proto = method_decl_in(c, cls, &name);
            let local = has_kw(c, t, "local") || proto.is_some_and(|p| has_kw(c, p, "local"));
            if !local || name == "new" || routine_is_virtual(c, t) {
                continue;
            }
            let Some(body) = c.tree.child(t, Body) else { continue };
            if crate::support::class_index::count_statements(c, body) == 0 || body_refs_if_good(c, body).is_none() {
                continue;
            }
            let used = methods.iter().filter(|&&m| m != t).any(|&m| {
                let Some(b) = c.tree.child(m, Body) else { return false };
                if body_refs_if_good(c, b).is_none() {
                    return false;
                }
                body_calls(c, b).into_iter().any(|call| {
                    crate::support::slang_ast::callee_decl(c, call).is_some_and(|d| {
                        last_name(c, d) == name && enclosing_class(c, d).is_some_and(|k| k == cls)
                    })
                })
            });
            if used {
                continue;
            }
            let Some(nm) = c.tree.child(t, Name) else { continue };
            let Some(loc) = c.tree.token_bounds(nm).map(|(_, b)| c.loc(b)) else { continue };
            out.add(
                "SV-DED-014",
                "UnusedLocalMethod",
                Severity::Warning,
                format!(
                    "Local method '{cname}::{name}' has no caller in the class. Remove it, or call it from the method that should use it."
                ),
                loc,
            );
        }
    }
}

/// The calls slang's visit of a body sees (see [`body_refs`]).
fn body_calls(c: &Cst, body: NodeId) -> Vec<NodeId> {
    crate::support::slang_ast::subtree(c, body)
        .into_iter()
        .filter(|&x| c.kind(x) == CallExpr)
        .filter(|&x| !crate::support::slang_ast::ancestors(c, x).take_while(|&a| a != body).any(|a| c.kind(a) == Declarator))
        .filter(|&x| !crate::support::procedural::is_system_call(c, x))
        .collect()
}
