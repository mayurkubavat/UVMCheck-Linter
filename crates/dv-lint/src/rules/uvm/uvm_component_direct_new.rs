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
        let Some((k, name)) = target else { continue };
        if !is_component(k) {
            continue;
        }
        let times = ast_times(c, nx);
        for _ in 0..times {
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
