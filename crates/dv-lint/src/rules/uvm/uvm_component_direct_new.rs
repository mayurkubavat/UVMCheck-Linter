//! UVM-FAC-001 (UvmComponentDirectNew): UVM components must be created using
//! the UVM factory (type_name::type_id::create) instead of direct new() calls
//! to support testbench overrides.

use crate::support::class_index::{type_name, Classes};
use crate::support::literals::ast_times;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

/// A class type as slang prints it: `name#(args)`, without a package.
fn class_type_text(c: &Cst, dt: NodeId) -> String {
    let text: String = c.toks(dt).map(|t| c.text_of(t)).collect();
    let head = text.split('#').next().unwrap_or("");
    match head.rfind("::") {
        Some(p) => text[p + 2..].to_string(),
        None => text,
    }
}

pub(crate) fn component_direct_new(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    let g = crate::support::class_metrics::globals(c);
    let is_component = |i: usize| classes.chain(i).iter().any(|(n, _)| n == "uvm_component");
    for nx in c.nodes(NewExpr) {
        // `new[n]` makes an array; `new obj` copies.
        if c.tok(nx, "[").is_some() || (c.tree.child(nx, ArgList).is_none() && c.tree.children(nx).next().is_some()) {
            continue;
        }
        let Some(p) = c.parent(nx) else { continue };
        let target = match c.kind(p) {
            BinaryExpr if c.tok(p, "=").is_some() && c.tree.children(p).nth(1) == Some(nx) => {
                let Some(l) = c.tree.children(p).next() else { continue };
                let cls = classes.enclosing(c, nx);
                let routine = crate::support::class_index::routine_of(c, nx);
                let env = crate::support::class_metrics::Env::new(c, &classes, cls, routine, &g);
                let k = env.expr_type(l).filter(|t| !t.array).and_then(|t| t.class);
                k.map(|k| {
                    (k, env.decl_type(l).map(|t| class_type_text(c, t)).unwrap_or_else(|| classes.list[k].name.clone()))
                })
            }
            Declarator => {
                let Some(dt) = c.parent(p).and_then(|d| c.tree.child(d, DataType)) else { continue };
                type_name(c, dt).and_then(|n| classes.find(&n).map(|k| (k, class_type_text(c, dt))))
            }
            _ => None,
        };
        let Some((k, name)) = target else {
            // `T obj; obj = new(...)` with T a type parameter: slang prints
            // the parameter (`pkg::cls::T`) in each specialization where it
            // is a component.
            for (loc, text) in type_param_targets(c, &classes, &is_component, nx) {
                s.add(
                    "UVM-FAC-001",
                    "UvmComponentDirectNew",
                    Severity::Error,
                    format!(
                        "Direct instantiation of uvm_component '{text}' using new(). Use type_id::create() to enable factory overrides."
                    ),
                    loc,
                );
            }
            continue;
        };
        if !is_component(k) {
            continue;
        }
        // The class as slang's `Type::toString()` prints `new`'s type: once
        // per specialization of a generic class around it, else per visit.
        let ix = c.index();
        let lhs = c.parent(nx).filter(|&p| c.kind(p) == BinaryExpr).and_then(|p| c.tree.children(p).next());
        let per = lhs.and_then(|l| crate::expr_types::spec_types(c).get(&(ix, l)).cloned());
        let names: Vec<String> = match per {
            Some(per) => per.iter().flatten().map(|(t, _)| t.text.clone()).collect(),
            None => {
                let typed = lhs.and_then(|l| crate::expr_types::expr_types(c).get(&(ix, l)).map(|t| t.text.clone()));
                vec![typed.unwrap_or(name); ast_times(c, nx)]
            }
        };
        for name in names {
            s.add(
                "UVM-FAC-001",
                "UvmComponentDirectNew",
                Severity::Error,
                format!(
                    "Direct instantiation of uvm_component '{name}' using new(). Use type_id::create() to enable factory overrides."
                ),
                c.loc_of(nx),
            );
        }
    }
}

/// `x = new(...)` where x is declared with a type parameter of the generic
/// class around it (`T obj`): one finding per specialization in which the
/// parameter is a component class, named as slang prints the type
/// (`pkg::cls::T`).
fn type_param_targets(
    c: &Cst,
    classes: &Classes,
    is_component: &dyn Fn(usize) -> bool,
    nx: NodeId,
) -> Vec<(crate::Loc, String)> {
    let Some(p) = c.parent(nx).filter(|&p| c.kind(p) == BinaryExpr && c.tok(p, "=").is_some()) else {
        return Vec::new();
    };
    let Some(l) = c.tree.children(p).next() else { return Vec::new() };
    let Some(per) = crate::expr_types::spec_types(c).get(&(c.index(), l)).cloned() else { return Vec::new() };
    per.iter()
        .flatten()
        .filter(|(_, canon)| {
            let base = canon.split('#').next().unwrap_or("");
            let base = base.rsplit("::").next().unwrap_or(base);
            // `new` of a virtual class does not bind, so neither does the
            // body (uvm_component_registry's default, T = uvm_component).
            classes.find(base).is_some_and(|k| {
                (classes.list[k].name == "uvm_component" || is_component(k))
                    && c.tok(classes.list[k].node, "virtual").is_none()
            })
        })
        .map(|(t, _)| (c.loc_of(nx), t.text.clone()))
        .collect()
}
