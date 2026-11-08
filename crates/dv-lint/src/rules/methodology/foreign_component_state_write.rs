//! UVM-CPL-003 (ForeignComponentStateWrite): Flags assignments to properties of
//! another component through a handle, except a parent setting up its own
//! subtree in new() and the build, connect and end_of_elaboration phases
//! (content coupling, MG-2.1).

use crate::support::class_roles::{is_component_role, role, role_name, Ctx, Index};
use crate::support::free_routines::{assignment_lhs, property_owner, skip_selects};
use crate::support::routine_bodies::bodies;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

fn is_this_or_super(c: &Cst, e: NodeId) -> bool {
    let x = skip_selects(c, e);
    c.kind(x) == NameExpr && matches!(c.tree.bare_text(x).as_str(), "this" | "super")
}

/// A descendant reached from one of the class's own (or inherited)
/// properties: `agt`, `agt[0]`, `this.agt`, `agt.drv` ...
fn reached_through_own_child(ctx: &Ctx, c: &Cst, e: NodeId) -> bool {
    let mut x = skip_selects(c, e);
    while c.kind(x) == MemberExpr {
        let Some(v) = c.tree.children(x).next() else { return false };
        if is_this_or_super(c, v) {
            break;
        }
        x = skip_selects(c, v);
    }
    let name = match c.kind(x) {
        NameExpr if c.tree.children(x).next().is_none() => c.tree.bare_text(x),
        MemberExpr => c.last_text(x).to_string(),
        _ => return false,
    };
    if c.kind(x) == NameExpr {
        if let Some(m) = ctx.m {
            if m.locals.iter().any(|l| l.name == name) || m.arguments.iter().any(|a| a.name == name) {
                return false;
            }
        }
    }
    property_owner(ctx.d, ctx.cls, &name).is_some()
}

pub(crate) fn foreign_component_state_write(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for (cls, m, c, body) in bodies(d, &ix) {
        let ctx = Ctx { d, cls, m: Some(m), td: Some(&ix.typedefs) };
        let build_time =
            matches!(m.name.as_str(), "build_phase" | "connect_phase" | "end_of_elaboration_phase" | "new");
        for n in c.subtree(body) {
            let Some(lhs) = assignment_lhs(c, n) else { continue };
            let target = skip_selects(c, lhs);
            if c.kind(target) != MemberExpr {
                continue;
            }
            let Some(obj) = c.tree.children(target).next() else { continue };
            if is_this_or_super(c, obj) {
                continue;
            }
            let Some(oc) = ctx.class_of(c, obj) else { continue };
            if !is_component_role(role(d, oc)) || property_owner(d, oc, c.last_text(target)).is_none() {
                continue;
            }
            if build_time && reached_through_own_child(&ctx, c, obj) {
                continue;
            }
            let ot = c.expr_text(obj);
            if ot == "p_sequencer" || ot == "m_sequencer" {
                continue;
            }
            out.add(
                "UVM-CPL-003",
                "ForeignComponentStateWrite",
                Severity::Warning,
                format!(
                    "'{}::{}' writes '{}', state that belongs to {} '{}'. The other component can no longer change how it keeps that state, and the write \
is invisible from its code. Send the value through its config object, a TLM port or a method it provides.",
                    cls.name,
                    m.name,
                    c.expr_text(target),
                    role_name(role(d, oc)),
                    oc.name
                ),
                c.loc_of(target),
            );
        }
    }
}
