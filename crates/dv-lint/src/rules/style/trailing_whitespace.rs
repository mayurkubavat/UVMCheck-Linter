//! SV-STY-015 (TrailingWhitespace): Disallows trailing whitespace (spaces and
//! tabs) at the end of lines in source files.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::source_layout::{at, buffers, lines};

pub(crate) fn trailing_whitespace(c: &Cst, s: &mut Sink) {
    for (f, text) in buffers(c) {
        if text.len() == 1 {
            continue;
        }
        let b = text.as_bytes();
        for (start, end, num) in lines(&text) {
            let mut ws = end;
            while ws > start && (b[ws - 1] == b' ' || b[ws - 1] == b'\t') {
                ws -= 1;
            }
            if ws < end {
                s.add(
                    "SV-STY-015",
                    "TrailingWhitespace",
                    Severity::Info,
                    format!(
                        "Line {num} contains {} trailing whitespace character(s). Trailing whitespace creates spurious git diffs and violates formatting hygiene.",
                        end - ws
                    ),
                    at(c, f, ws),
                );
            }
        }
    }
}
