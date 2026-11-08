//! SV-DED-010 (EmptyForkJoin): Disallows empty fork...join, fork...join_any, or
//! fork...join_none blocks that contain zero statements.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn empty_fork_join(c: &Cst, s: &mut Sink) {
    for n in c.nodes(ParBlock) {
        if c.items(n).into_iter().next().is_some() {
            continue;
        }
        let (Some(fork), Some(join)) =
            (c.tok(n, "fork"), c.toks(n).find(|&t| matches!(c.text_of(t), "join" | "join_any" | "join_none")))
        else {
            continue;
        };
        s.add(
            "SV-DED-010",
            "EmptyForkJoin",
            Severity::Warning,
            format!(
                "Parallel 'fork ... {}' block contains zero statements. Spawning empty concurrent fork blocks adds unnecessary delta cycles to the simulator event queue with zero functional behavior. Remove the empty block.",
                c.text_of(join)
            ),
            c.loc(fork),
        );
    }
}
