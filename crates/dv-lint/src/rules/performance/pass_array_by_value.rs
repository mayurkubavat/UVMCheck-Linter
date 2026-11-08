//! SV-PRF-001 (PassArrayByValue): Passing dynamic arrays, queues, associative
//! arrays, unpacked arrays, or unpacked structs by value (input) causes
//! expensive memory duplication on every call.

use crate::{Severity, Sink};
use dv_model::{Design, Direction, TypeShape};

pub(crate) fn pass_array_by_value(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.is_builtin {
                continue;
            }
            for a in &sub.arguments {
                if a.direction != Direction::In || a.shape == TypeShape::Other {
                    continue;
                }
                out.add(
                    "SV-PRF-001",
                    "PassArrayByValue",
                    Severity::Warning,
                    format!(
                        "Argument '{}' of type '{}' in method '{}' is passed by value (input). Use 'const ref' instead.",
                        a.name, a.canonical_type, sub.name
                    ),
                    a.loc,
                );
            }
        }
    }
}
