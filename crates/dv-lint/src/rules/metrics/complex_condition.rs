//! SV-MET-005 (ComplexCondition): Flags conditions with more && / || operators
//! than the threshold (default 5; Pylint R0916, Checkstyle
//! BooleanExpressionComplexity).

use crate::support::statements::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::routine_metrics::{each_routine, routine_metrics};

pub(crate) fn complex_condition(c: &Cst, s: &mut Sink) {
    let limit = 5;
    each_routine(c, |_, r| {
        let m = routine_metrics(c, r.body);
        if m.max_bool_ops <= limit {
            return;
        }
        let loc = m.most_complex.map(|n| start(c, n)).unwrap_or_else(|| c.loc(r.tok));
        report(
            s,
            r.times,
            "SV-MET-005",
            "ComplexCondition",
            Severity::Warning,
            format!(
                "This condition in '{}' combines {} && / || operators (limit {limit}). Name the parts with local \
                 bits or a helper function so the check can be reviewed.",
                r.name, m.max_bool_ops
            ),
            loc,
        );
    });
}
