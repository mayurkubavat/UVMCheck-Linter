//! SV-SVA-006 (ComplexInlineProperty): Flags assert/assume/cover property
//! statements whose inline property uses more than max_temporal_ops (default 3)
//! temporal operators (##, |->, |=>, [*], throughout, within, until, ...).

use crate::support::trivia::is_library_tok;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn complex_inline_property(c: &Cst, s: &mut Sink) {
    // Option `max_temporal_ops` (rules.SV-SVA-006.options).
    fn limit() -> usize {
        crate::options::get("SV-SVA-006", "max_temporal_ops", 3) as usize
    }
    const TEMPORAL: [&str; 15] = [
        "##",
        "|->",
        "|=>",
        "throughout",
        "within",
        "intersect",
        "until",
        "s_until",
        "until_with",
        "s_until_with",
        "implies",
        "#-#",
        "#=#",
        "nexttime",
        "s_nexttime",
    ];
    for n in c.nodes(ConcurrentAssert) {
        let Some(first) = c.first_tok(n) else { continue };
        if c.toks(n).next().is_some_and(|k| c.text_of(k) == "expect") {
            continue;
        }
        if is_library_tok(c, first) {
            continue;
        }
        let Some(spec) = c.tree.child(n, PropertyExpr) else { continue };
        let ops = c.tree.token_range(spec).iter().filter(|&&t| TEMPORAL.contains(&c.text_of(t))).count();
        if ops > limit() {
            s.add(
                "SV-SVA-006",
                "ComplexInlineProperty",
                Severity::Info,
                format!(
                    "This assertion writes {ops} temporal operators inline. Name the pieces as sequence/property \
                     declarations (s_addr_phase, s_data_phase) so the property reads like the protocol's timing diagram."
                ),
                c.loc(first),
            );
        }
    }
}
