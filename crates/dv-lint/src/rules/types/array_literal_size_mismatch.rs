//! SV-TYP-009 (ArrayLiteralSizeMismatch): Flags positional array assignment
//! patterns '{...} where the number of elements does not match the target fixed
//! array dimension.

use sv_syntax::SyntaxKind;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;

/// slang's WrongNumberAssignmentPatterns diagnostic, once per pattern.
pub(crate) fn array_literal_size(c: &Cst, s: &mut Sink) {
    for ap in c.nodes(AssignPattern) {
        if !crate::support::sema_facts::pattern_count_mismatch(c, ap) {
            continue;
        }
        s.add(
            "SV-TYP-009",
            "ArrayLiteralSizeMismatch",
            Severity::Warning,
            "Array literal assignment pattern element count does not match target array dimension. In IEEE 1800 \u{a7} 10.9 array assignment semantics, positional assignment patterns must provide an element count matching the target array dimension."
                .to_string(),
            c.loc_of(ap),
        );
    }
}
