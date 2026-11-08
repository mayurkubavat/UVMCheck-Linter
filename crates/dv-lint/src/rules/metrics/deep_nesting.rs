//! SV-MET-004 (DeepNesting): Flags routines whose control structures nest
//! deeper than the threshold (default 5; Checkstyle NestedIfDepth, clang-tidy
//! readability-function-size NestingThreshold).

use crate::support::routine_metrics::{each_routine, routine_metrics};
use crate::support::statements::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};

pub(crate) fn deep_nesting(c: &Cst, s: &mut Sink) {
    let limit = crate::options::get("SV-MET-004", "max_depth", 5) as _;
    each_routine(c, |_, r| {
        let m = routine_metrics(c, r.body);
        if m.max_nesting <= limit {
            return;
        }
        let loc = m.deepest.map(|n| start(c, n)).unwrap_or_else(|| c.loc(r.tok));
        report(
            s,
            r.times,
            "SV-MET-004",
            "DeepNesting",
            Severity::Warning,
            format!(
                "Control structures in '{}' nest {} levels deep here (limit {limit}). Return early or extract the \
                 inner loop into its own task.",
                r.name, m.max_nesting
            ),
            loc,
        );
    });
}
