//! SV-DLY-002 (IntraAssignmentDelay): Disallows intra-assignment timing
//! controls (e.g., a = #5 b; or a <= #5 b;) on procedural assignments.

use crate::syntax::{assignments, Cst};
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn intra_assignment_delay(c: &Cst, s: &mut Sink) {
    // slang gives `<=` its own kind, so only the blocking form is reached.
    for (op, _, rhs) in assignments(c) {
        if c.kind(rhs) == TimedExpr {
            s.add(
                "SV-DLY-002",
                "IntraAssignmentDelay",
                Severity::Error,
                "Intra-assignment delay used on blocking procedural assignment ('='). \
                 Blocking intra-assignment delays (e.g. 'a = #5 b;') sample the RHS, suspend thread execution for the delay, and update the LHS later. \
                 This creates severe simulation race conditions if RHS signals mutate in the interim. \
                 Use separate procedural delay statements ('#5; a = b;') or non-blocking clocking drives."
                    .into(),
                c.loc(op),
            );
        }
    }
}
