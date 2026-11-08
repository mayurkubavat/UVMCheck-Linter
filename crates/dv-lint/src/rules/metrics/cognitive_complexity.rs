//! SV-MET-001 (CognitiveComplexity): Flags routines whose Sonar cognitive
//! complexity (nesting-weighted branching, with fork/join counted as nesting)
//! exceeds the threshold (default 25; Sonar S3776).

use crate::support::routine_metrics::{each_routine, routine_metrics};
use crate::support::statements::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};

pub(crate) fn cognitive_complexity(c: &Cst, s: &mut Sink) {
    let limit = crate::options::get("SV-MET-001", "threshold", 25) as _;
    each_routine(c, |_, r| {
        let m = routine_metrics(c, r.body);
        if m.cognitive <= limit {
            return;
        }
        report(
            s,
            r.times,
            "SV-MET-001",
            "CognitiveComplexity",
            Severity::Warning,
            format!(
                "'{}' has cognitive complexity {} (limit {limit}). Deeply nested forks, loops and branches are \
                 hard to review and to cover; extract the nested parts into named tasks.",
                r.name, m.cognitive
            ),
            c.loc(r.tok),
        );
    });
}
