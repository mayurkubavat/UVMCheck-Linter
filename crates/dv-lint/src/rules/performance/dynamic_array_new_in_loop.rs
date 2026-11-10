//! SV-PRF-011 (DynamicArrayNewInLoop): Disallows dynamic array allocation
//! (new[...]) inside simulation loops to prevent heap contention and memory
//! churn.

use crate::support::ast::{kids, subtree};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

/// The loops whose body slang's loop visitors search: for, foreach,
/// while, repeat, forever (LoopStmt) and do-while; with the body.
fn loop_body(c: &Cst, n: NodeId) -> Option<NodeId> {
    match c.kind(n) {
        LoopStmt => kids(c, n).last().copied(),
        DoWhileStmt => kids(c, n).first().copied(),
        _ => None,
    }
}

/// `new [n]` and `new [n](init)` (slang's NewArrayExpression).
fn is_new_array(c: &Cst, n: NodeId) -> bool {
    c.kind(n) == NewExpr && c.tok(n, "[").is_some()
}

pub(crate) fn new_array_in_loop(c: &Cst, s: &mut Sink) {
    // Each enclosing loop reports again, as each loop's finder does.
    for l in c.nodes(LoopStmt).into_iter().chain(c.nodes(DoWhileStmt)) {
        let Some(body) = loop_body(c, l) else { continue };
        for n in subtree(c, body) {
            if !is_new_array(c, n) {
                continue;
            }
            let Some(at) = c.first_tok(n) else { continue };
            s.add(
                "SV-PRF-011",
                "DynamicArrayNewInLoop",
                Severity::Warning,
                "Dynamic array allocation 'new[...]' is invoked inside a simulation loop. \
                 In SystemVerilog performance optimization, allocating dynamic \
                 array heap storage inside simulation loops causes continuous heap memory churn, heap lock contention, and fragmentation. \
                 Pre-allocate array capacity prior to loop entry, or reuse an existing pre-allocated buffer/queue."
                    .into(),
                c.loc(at),
            );
        }
    }
}
