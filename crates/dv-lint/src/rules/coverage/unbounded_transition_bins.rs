//! SV-COV-005 (UnboundedTransitionBins): Transition sequences in coverpoint
//! bins must not specify unbounded repetitions (e.g., [*1:$], [->1:$]).

use crate::support::slang_ast::*;
use crate::support::sva_cov::bins_name;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind::*;

pub(crate) fn unbounded_transition(c: &Cst, s: &mut Sink) {
    for it in c.nodes(TransItem) {
        let toks: Vec<u32> = c.toks(it).collect();
        let Some(open) = toks.iter().position(|&t| c.text_of(t) == "[") else { continue };
        let Some(close) = toks.iter().rposition(|&t| c.text_of(t) == "]") else { continue };
        let text = span_text(c, toks[open], toks[close]);
        if !text.contains('$') {
            continue;
        }
        let bin = ancestors(c, it)
            .find(|&a| c.kind(a) == Bins)
            .and_then(|b| bins_name(c, b))
            .map(|x| x.0)
            .unwrap_or_else(|| "unnamed_bin".into());
        s.add(
            "SV-COV-005",
            "UnboundedTransitionBins",
            Severity::Warning,
            format!(
                "Coverage transition bin '{bin}' contains an unbounded repetition sequence '{text}'. Unbounded transition sequences ([*1:$], [->1:$]) require simulator coverage engines to track open threads indefinitely, triggering severe memory bloat and simulator slowness. Use a fixed upper bound."
            ),
            c.loc(toks[open]),
        );
    }
}
