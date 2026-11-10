//! UVM-PAT-001 (CallbackNotRegistered): Flags `uvm_do_callbacks(T, CB, ...) in
//! classes where no `uvm_register_cb(T, CB) registers that pair for T or a base
//! of T.

use crate::support::class_roles::{derives, user_classes, Ctx, Index};
use crate::support::routine_bodies::bodies;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::{Class, Design};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

/// The type arguments of `name#(A, B)` written in a data type or name.
fn type_args(c: &Cst, n: NodeId) -> Vec<String> {
    let Some(pl) = c.subtree(n).into_iter().find(|&x| c.kind(x) == ParamList) else { return Vec::new() };
    let Some(al) = c.tree.child(pl, ArgList) else { return Vec::new() };
    c.tree
        .children(al)
        .filter(|&a| c.kind(a) == Arg)
        .filter_map(|a| c.tree.children(a).next().map(|e| c.tree.bare_text(e)))
        .collect()
}

fn same_or_derived(d: &Design, a: &Class, b: &Class) -> bool {
    a.id == b.id || derives(d, a.id, b.id)
}

pub(crate) fn callback_not_registered(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    // `uvm_register_cb(T, CB): static m_register_cb_<CB> = uvm_callbacks#(T, CB)::m_register_pair().
    let mut regs: Vec<(&Class, &Class)> = Vec::new();
    for cls in user_classes(d) {
        let Some((c, decl)) = ix.decl(cls.loc) else { continue };
        if c.kind(decl) != ClassDecl {
            continue;
        }
        let ctx = Ctx { d, cls, m: None, td: Some(&ix.typedefs) };
        for dd in c.items(decl).into_iter().filter(|&x| c.kind(x) == DataDecl) {
            for dcl in c.tree.children(dd).filter(|&x| c.kind(x) == Declarator) {
                let Some(name) = c.tree.child(dcl, Name) else { continue };
                if !c.tree.bare_text(name).starts_with("m_register_cb_") {
                    continue;
                }
                let Some(init) = c.tok(dcl, "=").and_then(|eq| c.node_after(dcl, eq)) else { continue };
                let init = c.strip(init);
                if c.kind(init) != CallExpr {
                    continue;
                }
                let Some(callee) = c.tree.children(init).next() else { continue };
                if c.kind(callee) != ScopedExpr {
                    continue;
                }
                let Some(owner) = c.tree.children(callee).next() else { continue };
                if c.kind(owner) != NameExpr || !c.tree.bare_text(owner).starts_with("uvm_callbacks#") {
                    continue;
                }
                let args = type_args(c, owner);
                let (Some(t), Some(cb)) =
                    (args.first().and_then(|a| ctx.class_of_type(a)), args.get(1).and_then(|a| ctx.class_of_type(a)))
                else {
                    continue;
                };
                regs.push((t, cb));
            }
        }
    }
    for (cls, m, c, body) in bodies(d, &ix) {
        let ctx = Ctx { d, cls, m: Some(m), td: Some(&ix.typedefs) };
        for dd in c.subtree(body).into_iter().filter(|&x| c.kind(x) == DataDecl) {
            let Some(ty) = c.tree.child(dd, DataType) else { continue };
            if c.tree.children(ty).next().is_none() && !c.tree.bare_text(ty).starts_with("uvm_callback_iter") {
                continue;
            }
            let text = c.tree.bare_text(ty);
            if text.split('#').next() != Some("uvm_callback_iter") {
                continue;
            }
            let args = type_args(c, ty);
            let (Some(t), Some(cb)) =
                (args.first().and_then(|a| ctx.class_of_type(a)), args.get(1).and_then(|a| ctx.class_of_type(a)))
            else {
                continue;
            };
            if regs.iter().any(|(rt, rcb)| same_or_derived(d, t, rt) && same_or_derived(d, cb, rcb)) {
                continue;
            }
            for dcl in c.tree.children(dd).filter(|&x| c.kind(x) == Declarator) {
                let Some(name) = c.tree.child(dcl, Name) else { continue };
                out.add(
                    "UVM-PAT-001",
                    "CallbackNotRegistered",
                    Severity::Warning,
                    format!(
                        "'{}' runs callbacks of type '{}' for '{}', but no class registers the pair with `uvm_register_cb({}, {}). uvm_callbacks::add() rejects unregistered pairs, so these callbacks are \
never called. Add `uvm_register_cb({}, {}) to '{}'.",
                        cls.name, cb.name, t.name, t.name, cb.name, t.name, cb.name, t.name
                    ),
                    c.loc_of(name),
                );
            }
        }
    }
}
