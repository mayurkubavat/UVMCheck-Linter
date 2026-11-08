//! SV-COR-006 (CaseWithoutDefault): Flags 'case', 'casex', and 'casez'
//! statements that lack an explicit 'default:' fallback branch.

use crate::syntax::{case_statements, Cst};
use crate::{Severity, Sink};

pub(crate) fn case_without_default(c: &Cst, s: &mut Sink) {
    for (n, kw) in case_statements(c) {
        if c.tok(n, "default").is_none() {
            s.add(
                "SV-COR-006",
                "CaseWithoutDefault",
                Severity::Warning,
                format!(
                    "Statement '{}' lacks an explicit 'default:' fallback branch. In verification state decoders, bus monitors, and packet parsers, omitting a default branch causes unhandled transaction edge cases and inadvertent simulation latch behavior.",
                    c.text_of(kw)
                ),
                c.loc(kw),
            );
        }
    }
}
