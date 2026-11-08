//! SV-PRF-012 (UnboundedDynamicArrayInConstraint): Flags dynamic array or queue
//! size constraints with an upper bound (< or <=) lacking an explicit lower
//! bound.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;
use crate::support::cross_labels::{kids, last_line};

pub(crate) fn unbounded_size(c: &Cst, s: &mut Sink) {
    for n in c.nodes(ConstraintItem) {
        // slang's ExpressionConstraint: `[soft] expr;`.
        if !c.toks(n).all(|t| matches!(c.text_of(t), "soft" | ";")) || c.tok(n, ";").is_none() {
            continue;
        }
        let [e] = kids(c, n)[..] else { continue };
        let text = c.text(e);
        let expr = last_line(&text);
        if !expr.contains(".size()") {
            continue;
        }
        let upper = expr.contains('<');
        let lower = expr.contains('>') || expr.contains("inside") || expr.contains("==");
        let Some(t) = c.first_tok(e).filter(|_| upper && !lower) else { continue };
        s.add(
            "SV-PRF-012",
            "UnboundedDynamicArrayInConstraint",
            Severity::Warning,
            format!(
                "Dynamic array size constraint '{expr}' specifies an upper bound without an explicit non-negative lower bound (e.g. 'size() >= 0' or 'size() > 0'). \
                 In SystemVerilog constraint optimization (IEEE 1800 § 18.5.1), \
                 solvers must search full 32-bit signed integer negative spaces when lower bounds are omitted, resulting in \
                 exponential solver search space explosion. Constrain both bounds explicitly (e.g. 'size() inside {{[1:10]}}')."
            ),
            c.loc(t),
        );
    }
}
