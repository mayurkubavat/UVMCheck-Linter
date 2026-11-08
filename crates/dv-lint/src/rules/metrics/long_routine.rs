//! SV-MET-002 (LongRoutine): Flags routines with more statements than the
//! threshold (default 100; clang-tidy readability-function-size, Checkstyle
//! MethodLength).

use crate::support::class_index::*;
use crate::support::routine_metrics::each_routine;
use crate::support::statements::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};

pub(crate) fn long_routine(c: &Cst, s: &mut Sink) {
    let limit = 100;
    each_routine(c, |_, r| {
        let n = r.body.map(|b| count_statements(c, b)).unwrap_or(0);
        if n <= limit {
            return;
        }
        report(
            s,
            r.times,
            "SV-MET-002",
            "LongRoutine",
            Severity::Warning,
            format!(
                "'{}' has {n} statements (limit {limit}). Split it into smaller tasks or functions with one job each.",
                r.name
            ),
            c.loc(r.tok),
        );
    });
}
