//! SV-DBC-001 (InterfaceWithoutAssertions): Flags an interface with clocking
//! blocks or modports, so a protocol interface, that holds no assertions
//! (assert/assume/cover property, immediate asserts, property declarations or
//! checkers) and is not the target of a bind.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use std::collections::HashSet;
use svlang::syntax::SyntaxKind::*;
use svlang::syntax::{Child, NodeId};

pub(crate) fn interface_without_assertions(d: &Design, csts: &[Cst], out: &mut Sink) {
    let mut bound = HashSet::new();
    let mut candidates = Vec::new();
    for c in csts {
        for n in c.nodes(BindDirective) {
            // bind <target> <module> <instance>: the target is the first name.
            if let Some(t) = c.tree.children(n).next() {
                bound.insert(c.trimmed(t));
            }
        }
        for n in c.nodes(InterfaceDecl) {
            let (mut protocol, mut assertions) = (false, false);
            for x in c.subtree(n) {
                match c.kind(x) {
                    ClockingDecl | ModportDecl => protocol = true,
                    ConcurrentAssert | PropertyDecl => assertions = true,
                    AssertStmt => assertions = true,
                    Instantiation if is_checker_inst(csts, c, x) => assertions = true,
                    _ => {}
                }
            }
            if !protocol || assertions {
                continue;
            }
            let Some(name) = c.tree.child(n, Name) else { continue };
            let Some((_, last)) = c.tree.token_bounds(name) else { continue };
            candidates.push((c.tree.bare_text(name), c.loc(last)));
        }
    }
    for (name, loc) in candidates {
        let f = d.file_name(loc).to_string();
        let f = if f.is_empty() { String::new() } else { f };
        let base = f.rsplit(['/', '\\']).next().unwrap_or("");
        if bound.contains(&name) || base.starts_with("uvm_") {
            continue;
        }
        out.add(
            "SV-DBC-001",
            "InterfaceWithoutAssertions",
            Severity::Info,
            format!(
                "Interface '{name}' defines a protocol (clocking blocks or modports) but holds no assertions. Every testbench that reuses it re-checks the protocol by hand, or not at \
all. Write the protocol's rules into it as assert property statements, or bind a checker to it."
            ),
            loc,
        );
    }
}

/// An instantiation of a checker declared in one of the trees.
fn is_checker_inst(csts: &[Cst], c: &Cst, n: NodeId) -> bool {
    let Some(ty) = c.kids(n).iter().find_map(|k| match *k {
        Child::Node(x) => Some(x),
        Child::Token(_) => None,
    }) else {
        return false;
    };
    let name = c.tree.bare_text(ty);
    csts.iter().any(|t| {
        t.nodes(CheckerDecl).into_iter().any(|k| t.tree.child(k, Name).is_some_and(|x| t.tree.bare_text(x) == name))
    })
}
