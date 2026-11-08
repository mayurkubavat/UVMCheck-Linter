//! RAL-ADP-001 (IncompleteRegAdapter): Flags uvm_reg_adapter subclasses whose
//! bus2reg() never sets rw.status or whose reg2bus() never reads rw.kind.

use crate::support::class_roles::{uvm_base_name, Index};
use crate::support::routine_bodies::bodies;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

#[derive(Default)]
struct ArgUse {
    written: Vec<String>,
    read: Vec<String>,
    whole: bool,
}

fn is_arg(c: &Cst, e: NodeId, arg: &str) -> bool {
    let e = c.strip(e);
    c.kind(e) == NameExpr && c.tree.children(e).next().is_none() && c.tree.bare_text(e) == arg
}

fn arg_uses(c: &Cst, n: NodeId, arg: &str, u: &mut ArgUse) {
    if let Some(lhs) = crate::support::free_routines::assignment_lhs(c, n) {
        let l = c.strip(lhs);
        if c.kind(l) == MemberExpr && c.tree.children(l).next().is_some_and(|v| is_arg(c, v, arg)) {
            u.written.push(c.last_text(l).to_string());
            if let Some(rhs) = c.tree.children(n).nth(1) {
                arg_uses(c, rhs, arg, u);
            }
            return;
        }
        if is_arg(c, l, arg) {
            u.whole = true;
        }
    }
    match c.kind(n) {
        MemberExpr => {
            if c.tree.children(n).next().is_some_and(|v| is_arg(c, v, arg)) {
                u.read.push(c.last_text(n).to_string());
            }
        }
        CallExpr => {
            for a in c.call_args(n) {
                if let Some(e) = c.arg_expr(a) {
                    if is_arg(c, e, arg) {
                        u.whole = true;
                    }
                }
            }
        }
        _ => {}
    }
    for k in c.node_kids(n) {
        arg_uses(c, k, arg, u);
    }
}

pub(crate) fn incomplete_reg_adapter(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for (cls, m, c, body) in bodies(d, &ix) {
        if uvm_base_name(d, cls) != "uvm_reg_adapter" {
            continue;
        }
        let why = if m.name == "bus2reg" && m.arguments.len() >= 2 {
            let mut u = ArgUse::default();
            arg_uses(c, body, &m.arguments[1].name, &mut u);
            if u.whole || u.written.iter().any(|w| w == "status") {
                continue;
            }
            "bus2reg() never sets rw.status, so every register access is reported as UVM_IS_OK even when the bus returned an error. Copy the bus response \
status into rw.status."
        } else if m.name == "reg2bus" && !m.arguments.is_empty() {
            let mut u = ArgUse::default();
            arg_uses(c, body, &m.arguments[0].name, &mut u);
            if u.whole || u.read.iter().any(|w| w == "kind") {
                continue;
            }
            "reg2bus() never reads rw.kind, so reads and writes become the same bus transaction. Set the item's direction from rw.kind."
        } else {
            continue;
        };
        out.add(
            "RAL-ADP-001",
            "IncompleteRegAdapter",
            Severity::Warning,
            format!("Register adapter '{}': {why}", cls.name),
            m.loc,
        );
    }
}
