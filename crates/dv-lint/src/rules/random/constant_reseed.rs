//! SV-TST-001 (ConstantReseed): Flags srandom() or set_randstate() with a
//! constant in components, sequences and other non-test classes; every seed of
//! the regression then runs the same stimulus.

use std::collections::{HashMap, HashSet};
use sv_syntax::NodeId;
use crate::support::class_items::{call_name, class_decls, class_items, decl_name, declarators, kids, subtree,
    user_code};
use crate::support::data_items::{is_number_literal, strip_conv, unparen};
use crate::support::sema_facts::ast_ok;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;
use crate::support::type_text::{base_name, expr_text};

/// uvm base classes swe::classRole knows; the first one up the chain
/// decides the role.
const UVM_ROLES: &[&str] = &[
    "uvm_driver", "uvm_push_driver", "uvm_monitor", "uvm_push_sequencer", "uvm_agent", "uvm_scoreboard",
    "uvm_subscriber", "uvm_env", "uvm_test", "uvm_mem", "uvm_vreg", "uvm_report_catcher", "uvm_sequence",
    "uvm_sequence_base", "uvm_reg_sequence", "uvm_sequence_item", "uvm_transaction", "uvm_component", "uvm_root",
    "uvm_object", "uvm_void",
];

fn known_role(n: &str) -> bool {
    UVM_ROLES.contains(&n)
        || n.starts_with("uvm_sequencer")
        || n.starts_with("uvm_reg")
        || n.starts_with("uvm_callback")
        || n.starts_with("uvm_tlm")
}

fn is_test(c: &Cst, cls: NodeId, by_name: &HashMap<String, NodeId>) -> bool {
    let mut cur = cls;
    for _ in 0..64 {
        let Some(b) = base_name(c, cur) else { return false };
        if b.starts_with("uvm_") && known_role(&b) {
            return b == "uvm_test";
        }
        match by_name.get(&b) {
            Some(&k) => cur = k,
            None => return false,
        }
    }
    false
}

/// A value swe::isConstantExpr accepts: a literal, a parameter, or an
/// operator over those.
fn is_constant(c: &Cst, e: NodeId, params: &HashSet<String>) -> bool {
    let e = unparen(c, strip_conv(c, e).0);
    match c.kind(e) {
        Literal => is_number_literal(c, e) || c.trimmed(e).starts_with('\''),
        NameExpr => params.contains(&c.tree.bare_text(e)),
        BinaryExpr => kids(c, e).iter().all(|&k| is_constant(c, k, params)),
        _ => false,
    }
}

pub(crate) fn constant_reseed(c: &Cst, s: &mut Sink) {
    let classes: Vec<(NodeId, String)> = class_decls(c)
        .into_iter()
        .filter(|&k| user_code(c, k) && !c.tree.child(k, Header).is_some_and(|h| c.tok(h, "::").is_some()))
        .filter_map(|k| Some((k, decl_name(c, k)?.0)))
        .collect();
    let by_name: HashMap<String, NodeId> = classes.iter().map(|(k, n)| (n.clone(), *k)).collect();
    let mut params = HashSet::new();
    for d in c.nodes(ParamDecl) {
        for (_, v, _) in declarators(c, d) {
            params.insert(v);
        }
    }
    // Out-of-block bodies: class name -> routines.
    let mut outside: HashMap<String, Vec<NodeId>> = HashMap::new();
    for f in c.tree.children(c.tree.root).flat_map(|x| std::iter::once(x).chain(c.tree.children(x))) {
        if !matches!(c.kind(f), FunctionDecl | TaskDecl) {
            continue;
        }
        let Some(nm) = c.tree.child(f, Name) else { continue };
        let toks: Vec<u32> = c.tree.token_range(nm);
        if toks.len() == 3 && c.text_of(toks[1]) == "::" {
            outside.entry(c.text_of(toks[0]).to_string()).or_default().push(f);
        }
    }
    for (cls, cname) in &classes {
        if is_test(c, *cls, &by_name) {
            continue;
        }
        let mut routines: Vec<NodeId> =
            class_items(c, *cls).into_iter().filter(|&x| matches!(c.kind(x), FunctionDecl | TaskDecl)).collect();
        routines.extend(outside.get(cname).into_iter().flatten().copied());
        for f in routines {
            let Some(body) = c.tree.child(f, Body) else { continue };
            let m = c.tree.child(f, Name).map(|n| c.last_text(n).to_string()).unwrap_or_default();
            for call in subtree(c, body).into_iter().filter(|&x| c.kind(x) == CallExpr) {
                if !matches!(call_name(c, call), Some("srandom" | "set_randstate")) {
                    continue;
                }
                let Some(last) = c
                    .tree
                    .child(call, ArgList)
                    .and_then(|a| c.tree.children(a).filter(|&x| c.kind(x) == Arg).last())
                    .and_then(|a| c.tree.children(a).next())
                else {
                    continue;
                };
                if !is_constant(c, last, &params) || !ast_ok(c, call) {
                    continue;
                }
                let Some(at) = c.first_tok(call) else { continue };
                s.add(
                    "SV-TST-001",
                    "ConstantReseed",
                    Severity::Info,
                    format!(
                        "'{}' in {cname}::{m} reseeds with a constant, so this object produces the same random values \
                         for every simulation seed. Remove it, or derive the seed from the global seed (for example \
                         with get_full_name()).",
                        expr_text(c, call)
                    ),
                    c.loc(at),
                );
            }
        }
    }
}
