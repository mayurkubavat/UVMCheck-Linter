//! SV-MET-007 (LongTask): Flags tasks longer than the line budget (default 40
//! source lines, option max_lines).

use crate::support::statements::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::routine_metrics::{Routine, each_routine};

/// `metrics::routineLineSpan`: lines from the routine's name to the end of
/// its last statement.
fn line_span(c: &Cst, r: &Routine) -> i64 {
    let start = c.loc(r.tok);
    let Some(body) = r.body else { return 1 };
    let Some(last) = list(c, body).last().copied() else {
        return 1;
    };
    let Some((_, end)) = c.tree.token_bounds(last) else {
        return 1;
    };
    let end = c.loc(end);
    let (sf, sl) = start.expansion.unwrap_or((start.file, start.line));
    let (ef, el) = end.expansion.unwrap_or((end.file, end.line));
    if sf != ef {
        return 1;
    }
    if el >= sl {
        i64::from(el - sl + 1)
    } else {
        1
    }
}

pub(crate) fn long_task(c: &Cst, s: &mut Sink) {
    let limit = 40;
    each_routine(c, |_, r| {
        if !r.is_task {
            return;
        }
        let lines = line_span(c, r);
        if lines <= limit {
            return;
        }
        let place = if r.owner.is_empty() {
            r.name.clone()
        } else {
            format!("{}::{}", r.owner, r.name)
        };
        report(
            s,
            r.times,
            "SV-MET-007",
            "LongTask",
            Severity::Warning,
            format!("Task '{place}' is {lines} lines (limit {limit}). Split it into smaller tasks."),
            c.loc(r.tok),
        );
    });
}
