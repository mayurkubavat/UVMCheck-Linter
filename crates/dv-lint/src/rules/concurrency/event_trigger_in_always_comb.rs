//! SV-CON-006 (EventTriggerInAlwaysComb): Disallows event triggers ('->' or
//! '->>') inside always_comb blocks (IEEE 1800 § 9.2.2.2).

use sv_syntax::{NodeId, SyntaxKind};
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::procedural::{add_n, start};

fn proc_kind<'a>(c: &'a Cst, p: NodeId) -> &'a str {
    c.keyword(p).map_or("", |k| c.text_of(k))
}

pub(crate) fn trigger_in_comb(c: &Cst, s: &mut Sink) {
    for p in c.nodes(ProceduralBlock) {
        if proc_kind(c, p) != "always_comb" {
            continue;
        }
        let n = times(c, p);
        for x in subtree(c, p) {
            if c.kind(x) == EventTrigger {
                if let Some(loc) = start(c, x) {
                    add_n(
                        s,
                        n,
                        "SV-CON-006",
                        "EventTriggerInAlwaysComb",
                        Severity::Error,
                        "Event trigger inside 'always_comb' block causes simulation delta-cycle oscillation and simulator lockup. Per IEEE 1800 § 9.2.2.2, combinational blocks must be purely functional and sensitive only to signal reads; triggering events produces infinite scheduling loops.".into(),
                        loc,
                    );
                }
            }
        }
    }
}
