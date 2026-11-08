//! SV-STY-016 (MaxLineLength): Flags source lines exceeding 120 characters
//! (option: max_columns) to preserve readability across split-view editors and
//! terminal reviews.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::source_layout::{at, buffers, lines};

pub(crate) fn max_line_length(c: &Cst, s: &mut Sink) {
    const MAX: usize = 120;
    for (f, text) in buffers(c) {
        if text.len() == 1 {
            continue;
        }
        for (start, end, num) in lines(&text) {
            let len = end - start;
            if len > MAX {
                s.add(
                    "SV-STY-016",
                    "MaxLineLength",
                    Severity::Info,
                    format!(
                        "Line {num} length ({len} characters) exceeds maximum recommended threshold of {MAX} characters. Wrap long expressions or break statements across lines."
                    ),
                    at(c, f, start + MAX),
                );
            }
        }
    }
}
