//! SV-PRF-013 (ForkInClockedLoop): Flags fork/join, join_any, and join_none
//! inside a clocked loop (always_ff, always @(posedge/negedge), or a loop that
//! waits on a clock edge).

use sv_syntax::{NodeId, SyntaxKind};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;

/// A clock edge that gates `n`: a posedge/negedge event outside any fork.
fn has_edge_outside_fork(c: &Cst, n: NodeId) -> bool {
    if c.kind(n) == ParBlock {
        return false;
    }
    if c.kind(n) == EventExpr && c.toks(n).any(|t| matches!(c.text_of(t), "posedge" | "negedge")) {
        return true;
    }
    c.tree.children(n).any(|k| has_edge_outside_fork(c, k))
}

fn in_clocked_loop(c: &Cst, fork: NodeId) -> bool {
    let mut cur = c.parent(fork);
    while let Some(p) = cur {
        let kw = c.keyword(p).map(|t| c.text_of(t)).unwrap_or("");
        match c.kind(p) {
            ProceduralBlock if kw == "always_ff" => return true,
            ProceduralBlock if kw == "always" && has_edge_outside_fork(c, p) => return true,
            LoopStmt | DoWhileStmt if has_edge_outside_fork(c, p) => return true,
            _ => {}
        }
        cur = c.parent(p);
    }
    false
}

pub(crate) fn fork_in_clocked_loop(c: &Cst, s: &mut Sink) {
    for n in c.nodes(ParBlock) {
        let Some(fork) = c.tok(n, "fork") else { continue };
        if !in_clocked_loop(c, n) {
            continue;
        }
        let join = c
            .toks(n)
            .find(|&t| matches!(c.text_of(t), "join" | "join_any" | "join_none"))
            .map(|t| c.text_of(t))
            .unwrap_or("");
        s.add(
            "SV-PRF-013",
            "ForkInClockedLoop",
            Severity::Warning,
            format!(
                "fork ... {join} sits inside a clocked loop, so every clock starts another thread. \
                 Do the work in the clocked process, or start the fork once outside the loop."
            ),
            c.loc(fork),
        );
    }
}
