//! UVM-CFG-009 (SharedConfigMutatedAtRuntime): Flags writes to fields of a
//! config object that the class got from uvm_config_db and did not create, made
//! outside new() and the build phases.

use crate::support::class_roles::{class_methods, key, user_classes, Ctx, Index};
use crate::support::free_routines::{assignment_lhs, skip_selects};
use crate::support::routine_bodies::{factory_create_type, is_config_db_call, Calls};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::{ClassId, Design, Method};
use std::collections::{HashMap, HashSet};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

fn is_build_time(m: &str) -> bool {
    matches!(m, "new" | "build_phase" | "connect_phase" | "end_of_elaboration_phase" | "start_of_simulation_phase")
}

/// A variable or member, as `swe::referencedSymbol` names it: a local or
/// argument of one method, or a property of one class.
#[derive(Clone, PartialEq, Eq, Hash)]
enum Sym {
    Local((u32, u32, u32, Option<(u32, u32)>), String),
    Prop(ClassId, String),
}

fn name_sym(ctx: &Ctx, m: &Method, name: &str) -> Option<Sym> {
    if m.locals.iter().any(|l| l.name == name) || m.arguments.iter().any(|a| a.name == name) {
        return Some(Sym::Local(key(m.loc), name.to_string()));
    }
    std::iter::once(ctx.cls)
        .chain(ctx.d.ancestors(ctx.cls.id))
        .find(|k| k.properties.iter().any(|p| p.name == name))
        .map(|k| Sym::Prop(k.id, name.to_string()))
}

fn is_this(c: &Cst, n: NodeId) -> bool {
    c.kind(n) == NameExpr && matches!(c.tree.bare_text(n).as_str(), "this" | "super")
}

fn referenced_sym(ctx: &Ctx, m: &Method, c: &Cst, e: NodeId) -> Option<Sym> {
    let e = c.strip(e);
    match c.kind(e) {
        NameExpr if c.tree.children(e).next().is_none() => name_sym(ctx, m, &c.tree.bare_text(e)),
        MemberExpr => {
            let obj = c.tree.children(e).next()?;
            let member = c.last_text(e);
            if is_this(c, obj) {
                return name_sym(ctx, m, member);
            }
            let oc = ctx.class_of(c, obj)?;
            std::iter::once(oc)
                .chain(ctx.d.ancestors(oc.id))
                .find(|k| k.properties.iter().any(|p| p.name == member))
                .map(|k| Sym::Prop(k.id, member.to_string()))
        }
        _ => None,
    }
}

fn root_sym(ctx: &Ctx, m: &Method, c: &Cst, e: NodeId) -> Option<Sym> {
    let mut x = c.strip(e);
    loop {
        match c.kind(x) {
            SelectExpr => x = c.strip(c.tree.children(x).next()?),
            MemberExpr => {
                let obj = c.strip(c.tree.children(x).next()?);
                if is_this(c, obj) {
                    return name_sym(ctx, m, c.last_text(x));
                }
                x = obj;
            }
            NameExpr => return name_sym(ctx, m, &c.tree.bare_text(x)),
            _ => return None,
        }
    }
}

pub(crate) fn shared_config_mutated_at_runtime(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for cls in user_classes(d) {
        let methods: Vec<&Method> = class_methods(cls).collect();
        let (mut fetched, mut created) = (HashSet::new(), HashSet::new());
        for m in &methods {
            let Some((c, body)) = ix.body(m) else { continue };
            let ctx = Ctx { d, cls, m: Some(m), td: Some(&ix.typedefs) };
            let calls = Calls::new(m);
            for n in c.subtree(body) {
                if c.kind(n) == CallExpr && is_config_db_call(&calls, c, n, "get") {
                    let args = c.call_args(n);
                    if args.len() >= 4 {
                        if let Some(t) = c.arg_expr(args[3]) {
                            if ctx.class_of(c, t).is_some() {
                                if let Some(s) = referenced_sym(&ctx, m, c, t) {
                                    fetched.insert(s);
                                }
                            }
                        }
                    }
                }
                if let Some(lhs) = assignment_lhs(c, n) {
                    let Some(rhs) = c.tree.children(n).nth(1).map(|r| c.strip(r)) else { continue };
                    let creates = c.kind(rhs) == NewExpr
                        || (c.kind(rhs) == CallExpr && factory_create_type(&ctx, &calls, c, rhs).is_some())
                        || (c.kind(rhs) == CallExpr
                            && c.tree
                                .children(rhs)
                                .next()
                                .is_some_and(|k| c.kind(k) == ScopedExpr && c.last_text(k) == "new"));
                    if creates {
                        if let Some(s) = referenced_sym(&ctx, m, c, skip_selects(c, lhs)) {
                            created.insert(s);
                        }
                    }
                }
            }
        }
        let shared: HashSet<Sym> = fetched.difference(&created).cloned().collect();
        if shared.is_empty() {
            continue;
        }
        let build_only = build_time_helpers(cls, &methods);
        for m in &methods {
            if is_build_time(&m.name) || build_only.contains(&m.name.as_str()) {
                continue;
            }
            let Some((c, body)) = ix.body(m) else { continue };
            let ctx = Ctx { d, cls, m: Some(m), td: Some(&ix.typedefs) };
            for n in c.subtree(body) {
                let Some(lhs) = assignment_lhs(c, n) else { continue };
                let l = skip_selects(c, lhs);
                if c.kind(l) != MemberExpr {
                    continue;
                }
                // The member must be a class property.
                let Some(obj) = c.tree.children(l).next() else { continue };
                let member = c.last_text(l);
                let owner = if is_this(c, obj) { Some(cls) } else { ctx.class_of(c, obj) };
                let Some(owner) = owner else { continue };
                if !std::iter::once(owner)
                    .chain(d.ancestors(owner.id))
                    .any(|k| k.properties.iter().any(|p| p.name == member))
                {
                    continue;
                }
                let Some(root) = root_sym(&ctx, m, c, l) else { continue };
                if !shared.contains(&root) {
                    continue;
                }
                out.add(
                    "UVM-CFG-009",
                    "SharedConfigMutatedAtRuntime",
                    Severity::Warning,
                    format!(
                        "'{}::{}' writes '{}' in a config object it got from uvm_config_db. Every component that read the same object sees the change at an unknown time. Let \
the owner that created the object change it, or copy it before changing it.",
                        cls.name,
                        m.name,
                        c.expr_text(l)
                    ),
                    c.loc_of(l),
                );
            }
        }
    }
}

/// Methods only called from new() and the build phases (directly or
/// through other such helpers).
fn build_time_helpers<'m>(cls: &dv_model::Class, methods: &[&'m Method]) -> HashSet<&'m str> {
    let own: HashSet<&str> = methods.iter().map(|m| m.name.as_str()).collect();
    let mut callers: HashMap<&str, HashSet<&str>> = HashMap::new();
    for m in methods {
        for k in &m.calls {
            if !k.is_system && k.callee_scope == cls.name && own.contains(k.name.as_str()) {
                callers.entry(k.name.as_str()).or_default().insert(m.name.as_str());
            }
        }
    }
    let mut out: HashSet<&str> = HashSet::new();
    let mut changed = true;
    while changed {
        changed = false;
        for m in methods {
            let n = m.name.as_str();
            if out.contains(n) || is_build_time(n) || !callers.contains_key(n) {
                continue;
            }
            if callers[n].iter().all(|c| is_build_time(c) || out.contains(c)) {
                out.insert(n);
                changed = true;
            }
        }
    }
    out
}
