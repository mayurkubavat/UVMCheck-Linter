//! SV-SVA-001 (ImmediateAssertInCombinatorialLogic): Simple immediate
//! assertions in always_comb or combinatorial blocks evaluate transient delta
//! glitches.

use crate::support::procedural::start;
use crate::support::slang_ast::*;
use crate::support::sva_cov::add_n;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

fn proc_kind<'a>(c: &'a Cst, p: NodeId) -> &'a str {
    c.keyword(p).map_or("", |k| c.text_of(k))
}

pub(crate) fn immediate_in_comb(c: &Cst, s: &mut Sink) {
    for p in c.nodes(ProceduralBlock) {
        if !matches!(proc_kind(c, p), "always_comb" | "always_latch") {
            continue;
        }
        let n = times(c, p);
        for a in subtree(c, p) {
            if c.kind(a) != AssertStmt {
                continue;
            }
            let toks = tok_texts(c, a);
            if toks.contains(&"final") || toks.contains(&"#") {
                continue;
            }
            if let Some(loc) = start(c, a) {
                add_n(
                    s,
                    n,
                    "SV-SVA-001",
                    "ImmediateAssertInCombinatorialLogic",
                    Severity::Warning,
                    "Simple immediate assertion 'assert (...)' evaluated inside combinatorial process block. \
                     Immediate assertions in active combinatorial logic evaluate transient intermediate states (glitches) \
                     before signal values settle, triggering spurious false-positive failures. Use deferred assertion 'assert #0 (...)' or 'assert final (...)'."
                        .into(),
                    loc,
                );
            }
        }
    }
}
