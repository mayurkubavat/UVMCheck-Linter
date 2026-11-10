//! SV-OOP-010 (ClassInstantiationInLoop): Disallows direct class instantiation
//! via new() inside simulation loops to prevent heap contention and memory
//! allocation overhead.

use crate::support::spans::child_nodes;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

/// A `new` that slang reads as a class instantiation: not an array or a
/// copy, and not a call argument.
fn is_class_new(c: &Cst, n: NodeId) -> bool {
    c.tok(n, "[").is_none()
        && c.tree.children(n).all(|k| c.kind(k) == ArgList)
        && !c.parent(n).is_some_and(|p| c.kind(p) == Arg)
}

pub(crate) fn new_in_loop(c: &Cst, s: &mut Sink) {
    let mut loops = c.nodes(LoopStmt);
    loops.extend(c.nodes(DoWhileStmt));
    loops.sort_by_key(|n| c.first_tok(*n));
    for l in loops {
        let Some(kw) = c.keyword(l) else { continue };
        let kids = child_nodes(c, l);
        let body = match c.text_of(kw) {
            "for" | "foreach" | "while" | "repeat" => kids.last(),
            "do" => kids.first(),
            _ => None,
        };
        let Some(&body) = body else { continue };
        let mut stack = vec![body];
        let mut found = Vec::new();
        while let Some(x) = stack.pop() {
            if c.kind(x) == NewExpr && is_class_new(c, x) {
                found.push(x);
            }
            stack.extend(c.tree.children(x));
        }
        found.sort_by_key(|n| c.first_tok(*n));
        for n in found {
            let Some(t) = c.first_tok(n) else { continue };
            s.add(
                "SV-OOP-010",
                "ClassInstantiationInLoop",
                Severity::Warning,
                "Direct object instantiation with 'new()' inside a simulation loop. Repeatedly creating dynamic heap objects in simulation loops triggers frequent garbage collection and memory manager lock contention. Preallocate objects outside the loop or reuse transaction instances from an object pool.".to_string(),
                c.loc(t),
            );
        }
    }
}
