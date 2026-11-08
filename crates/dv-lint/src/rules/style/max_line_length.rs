//! SV-STY-016 (MaxLineLength): Flags source lines exceeding 120 characters
//! (option: max_columns) to preserve readability across split-view editors and
//! terminal reviews.

use crate::support::source_layout::{at, buffers, lines};
use crate::syntax::Cst;
use crate::{Severity, Sink};

pub(crate) fn max_line_length(c: &Cst, s: &mut Sink) {
    // Option `max_columns`.
    let max = crate::options::get("SV-STY-016", "max_columns", 120).max(1) as usize;
    for (f, text) in buffers(c) {
        if text.len() == 1 {
            continue;
        }
        for (start, end, num) in lines(&text) {
            let len = end - start;
            if len > max {
                s.add(
                    "SV-STY-016",
                    "MaxLineLength",
                    Severity::Info,
                    format!(
                        "Line {num} length ({len} characters) exceeds maximum recommended threshold of {max} characters. Wrap long expressions or break statements across lines."
                    ),
                    at(c, f, start + max),
                );
            }
        }
    }
}
