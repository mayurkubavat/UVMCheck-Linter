//! SV-RND-023 (UnallocatedRandObjectArray): Flags rand arrays of class handles
//! whose elements are never created. randomize() sizes a dynamic array but does
//! not construct objects, so every element stays null and its rand members are
//! never randomized.

use crate::support::class_items::{declarators, is_parameterized, qualifiers};
use crate::support::constraints::class_names;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::{HashMap, HashSet};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

pub(crate) fn unallocated_rand_array(c: &Cst, s: &mut Sink) {
    let classes: HashMap<String, NodeId> = class_names(c);
    // Arrays an element of which is assigned or pushed anywhere, by name.
    let mut populated: HashSet<String> = HashSet::new();
    let last_name = |x: NodeId| match c.kind(x) {
        NameExpr | MemberExpr => Some(c.last_text(x).to_string()),
        _ => None,
    };
    for n in c.nodes(BinaryExpr) {
        if !matches!(crate::support::class_items::op(c, n), "=" | "<=") {
            continue;
        }
        let Some(l) = c.tree.children(n).next().filter(|&l| c.kind(l) == SelectExpr) else { continue };
        if let Some(v) = c.tree.children(l).next().and_then(last_name) {
            populated.insert(v);
        }
    }
    for call in c.nodes(CallExpr) {
        if !matches!(crate::support::class_items::call_name(c, call), Some("push_back" | "push_front" | "insert")) {
            continue;
        }
        let Some(callee) = c.tree.children(call).next().filter(|&x| c.kind(x) == MemberExpr) else { continue };
        if let Some(v) = c.tree.children(callee).next().and_then(last_name) {
            populated.insert(v);
        }
    }
    for d in c.nodes(DataDecl) {
        let q = qualifiers(c, d);
        if !q.contains(&"rand") && !q.contains(&"randc") {
            continue;
        }
        let Some(cls) = c.parent(d).filter(|&p| c.kind(p) == ClassDecl) else { continue };
        if is_parameterized(c, cls) {
            continue;
        }
        let Some(dt) = c.tree.child(d, DataType) else { continue };
        if !crate::support::type_text::is_class_type(c, Some(dt), &classes) {
            continue;
        }
        for (dcl, name, tok) in declarators(c, d) {
            if !c.tree.children(dcl).any(|x| c.kind(x) == Dim) || populated.contains(&name) {
                continue;
            }
            s.add(
                "SV-RND-023",
                "UnallocatedRandObjectArray",
                Severity::Error,
                format!(
                    "rand array '{name}' holds class handles, but no element is ever created (no '{name}[i] = new' or \
                     type_id::create). randomize() leaves the elements null, so their contents are never randomized. \
                     Create the elements, for example in pre_randomize() after sizing the array."
                ),
                c.loc(tok),
            );
        }
    }
}
