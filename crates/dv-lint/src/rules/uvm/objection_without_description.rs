//! UVM-OBJ-005 (ObjectionWithoutDescription): Flags
//! raise_objection()/drop_objection() on a phase or objection with no
//! description or an empty one.

use crate::support::uvm_classes::{call_name, calls_in, receiver, string_value, At, Ix};
use crate::support::uvm_syntax::{arg_list, decl_name, ordered_expr, params};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::{ClassId, Design};
use sv_syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

/// Name of the nearest uvm_* ancestor.
fn uvm_base_name(d: &Design, id: ClassId) -> &str {
    d.ancestors(id).take(64).find(|a| a.name.starts_with("uvm_")).map(|a| a.name.as_str()).unwrap_or("")
}

pub(crate) fn objection_without_description(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for (ti, c) in csts.iter().enumerate() {
        for n in calls_in(c, c.tree.root) {
            let name = call_name(c, n);
            if name != "raise_objection" && name != "drop_objection" {
                continue;
            }
            if user_code_skip(&ix, c, n) {
                continue;
            }
            let Some(recv) = receiver(c, n) else { continue };
            let cx = ix.ctx(At { tree: ti, node: n });
            let Some(k) = ix.expr_class(c, &cx, recv, 0) else { continue };
            let kc = d.class(k);
            let base = uvm_base_name(d, k);
            if !(kc.name == "uvm_phase" || kc.name == "uvm_objection" || base == "uvm_phase" || base == "uvm_objection")
            {
                continue;
            }
            // The second argument is the description; an empty literal counts as none.
            let list = arg_list(c, n);
            let ps = list.map(|l| params(c, l)).unwrap_or_default();
            let named = ps.iter().any(|p| p.is_some_and(|a| ordered_expr(c, a).is_none()) || p.is_none());
            let has = if named {
                true
            } else if ps.len() < 2 {
                false
            } else {
                match ps[1].and_then(|a| ordered_expr(c, a)) {
                    Some(e) => string_value(c, e).is_none_or(|v| !v.is_empty()),
                    None => true,
                }
            };
            if has {
                continue;
            }
            let Some(t) = c.first_tok(n) else { continue };
            s.add(
                "UVM-OBJ-005",
                "ObjectionWithoutDescription",
                Severity::Info,
                format!(
                    "{name}() has no description. +UVM_OBJECTION_TRACE then shows only the \
                     component, not what it is waiting for. Pass one, e.g. \
                     phase.{name}(this, \"main traffic sequence\")."
                ),
                c.loc(t),
            );
        }
    }
}

/// `swe::UserCodeVisitor` skips the UVM library and the test stub.
fn user_code_skip(ix: &Ix, c: &Cst, n: NodeId) -> bool {
    let mut cur = Some(n);
    while let Some(p) = cur {
        match c.kind(p) {
            ClassDecl if decl_name(c, p).starts_with("uvm_") => return true,
            PackageDecl if decl_name(c, p) == "uvm_pkg" => return true,
            _ => {}
        }
        cur = c.parent(p);
    }
    let Some(t) = c.first_tok(n) else { return false };
    let l = c.loc(t);
    let f = ix.d.files().get(l.expansion.map_or(l.file, |e| e.0) as usize).map(String::as_str).unwrap_or("");
    f.contains("uvm_mock.svh") || f.contains("uvm_pkg.sv")
}
