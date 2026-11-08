//! SV-DED-009 (EmptyAlwaysOrInitial): Disallows initial, always, always_comb,
//! always_ff, always_latch, or final procedural blocks with empty bodies.

use sv_syntax::{NodeId, SyntaxKind};
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::syntax::{Cst, procedural_blocks};

fn effectively_empty(c: &Cst, stmt: Option<NodeId>) -> bool {
    let Some(n) = stmt else { return true };
    match c.kind(n) {
        EmptyStmt => true,
        SeqBlock | ParBlock => c.items(n).into_iter().next().is_none(),
        TimedStmt => effectively_empty(c, c.items(n).into_iter().find(|&x| c.kind(x) != TimingControl)),
        _ => false,
    }
}

pub(crate) fn empty_procedural_block(c: &Cst, s: &mut Sink) {
    for (n, kw) in procedural_blocks(c) {
        if effectively_empty(c, c.items(n).into_iter().next()) {
            s.add(
                "SV-DED-009",
                "EmptyAlwaysOrInitial",
                Severity::Warning,
                format!(
                    "Procedural '{}' block contains an empty body. Empty procedural blocks consume simulator scheduling cycles with zero functional impact and frequently indicate unfinished or abandoned logic.",
                    c.text_of(kw)
                ),
                c.loc(kw),
            );
        }
    }
}
