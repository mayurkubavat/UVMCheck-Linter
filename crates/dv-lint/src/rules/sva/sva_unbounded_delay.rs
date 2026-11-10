//! SVA-CYC-001 (SvaUnboundedDelay): Disallow unbounded cycle delays (##[1:$])
//! in SVA sequences.

use crate::support::spans::span;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind::*;

pub(crate) fn unbounded_delay(c: &Cst, s: &mut Sink) {
    for n in c.nodes(TimingControl) {
        let Some(first) = c.keyword(n) else { continue };
        let hash = c.text_of(first);
        if hash != "#" && hash != "##" {
            continue;
        }
        let Some((_, last)) = c.tree.token_bounds(n) else { continue };
        let shown = if let Some(open) = c.tok(n, "[") {
            // A sequence's `##[a:b]` (slang's DelayedSequenceElement).
            let Some(close) = c.tok(n, "]") else { continue };
            if close <= open + 1 || !c.parent(n).is_some_and(|p| c.kind(p) == SequenceExpr) {
                continue;
            }
            format!("[{}]", span(c, open + 1, close - 1))
        } else {
            // A statement's delay (slang's DelaySyntax).
            if !c.parent(n).is_some_and(|p| matches!(c.kind(p), TimedStmt | TimedExpr)) || last <= first {
                continue;
            }
            span(c, first + 1, last)
        };
        if !shown.contains('$') {
            continue;
        }
        s.add(
            "SVA-CYC-001",
            "SvaUnboundedDelay",
            Severity::Warning,
            format!(
                "Unbounded delay cycle range '##{shown}' in assertion sequence. \
                 In simulation, unbounded delays keep evaluation threads alive indefinitely in memory waiting for unreachable events. \
                 In formal property verification (FPV), unbounded delays cause infinite induction unrolling and proof timeouts. \
                 Specify a bounded maximum delay (e.g. '##[1:MAX_CYCLES]') or use the 's_eventually' liveness operator."
            ),
            c.loc(first),
        );
    }
}
