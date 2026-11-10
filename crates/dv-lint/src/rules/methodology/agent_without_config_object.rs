//! UVM-RUS-002 (AgentWithoutConfigObject): Flags agents and envs that read
//! three or more separate scalar settings from uvm_config_db instead of one
//! config object (Fowler 'data clumps', MG-4.2).

use crate::support::class_roles::{class_methods, role, role_name, user_classes, Ctx, Index, Role};
use crate::support::routine_bodies::{calls_in, is_config_db_call, Calls};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use std::collections::BTreeSet;
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

/// RUS-002's value types it leaves out: a class handle, a virtual
/// interface or an unpacked array.
fn is_handle_or_array(ctx: &Ctx<'_, '_>, c: &Cst, e: NodeId) -> bool {
    let e = c.strip(e);
    if ctx.class_of(c, e).is_some() {
        return true;
    }
    let name = match c.kind(e) {
        NameExpr if c.tree.children(e).next().is_none() => c.tree.bare_text(e),
        _ => return false,
    };
    let aggregate = |ty: &str| ty.contains('$') || ty.starts_with("virtual");
    if let Some(m) = ctx.m {
        if let Some(l) = m.locals.iter().find(|l| l.name == name) {
            return l.is_virtual_interface || aggregate(&l.ty);
        }
        if let Some(a) = m.arguments.iter().find(|a| a.name == name) {
            return aggregate(&a.ty);
        }
    }
    std::iter::once(ctx.cls)
        .chain(ctx.d.ancestors(ctx.cls.id))
        .find_map(|k| k.properties.iter().find(|p| p.name == name))
        .is_some_and(|p| p.is_virtual_interface || aggregate(&p.ty))
}

pub(crate) fn agent_without_config_object(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    // Option `max_scalar_fields` (rules.UVM-RUS-002.options).
    fn limit() -> usize {
        crate::options::get("UVM-RUS-002", "max_scalar_fields", 2) as usize
    }
    for cls in user_classes(d) {
        let r = role(d, cls);
        if r != Role::Agent && r != Role::Env {
            continue;
        }
        let mut fields = BTreeSet::new();
        let mut first = None;
        for m in class_methods(cls) {
            let Some((c, body)) = ix.body(m) else { continue };
            let ctx = Ctx { d, cls, m: Some(m), td: Some(&ix.typedefs) };
            let calls = Calls::new(m);
            for call in calls_in(c, body) {
                let args = c.call_args(call);
                if !is_config_db_call(&calls, c, call, "get") || args.len() < 4 {
                    continue;
                }
                let (Some(field), Some(value)) = (c.arg_expr(args[2]), c.arg_expr(args[3])) else { continue };
                if is_handle_or_array(&ctx, c, value) {
                    continue;
                }
                let f = c.string_value(field).unwrap_or_else(|| c.expr_text(field));
                if fields.insert(f) && fields.len() == 1 {
                    first = Some(c.loc_of(call));
                }
            }
        }
        if fields.len() <= limit() {
            continue;
        }
        let list = fields.iter().cloned().collect::<Vec<_>>().join(", ");
        out.add(
            "UVM-RUS-002",
            "AgentWithoutConfigObject",
            Severity::Warning,
            format!(
                "{} '{}' reads {} separate settings from uvm_config_db ({}). Settings that travel together belong in one config object: it is typed, \
documented in one place, can be randomized and is set with one call.",
                role_name(r),
                cls.name,
                fields.len(),
                list
            ),
            first.unwrap_or(cls.loc),
        );
    }
}
