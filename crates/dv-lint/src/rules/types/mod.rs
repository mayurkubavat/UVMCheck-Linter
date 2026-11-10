//! The types rules: the Rust twins of `cpp/src/rules/types/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::profile::*;
use crate::{Check, Rule, Severity};

mod array_literal_size_mismatch;
mod mixed_sign_comparison;
mod redundant_cast_to_same_type;
mod task_with_non_void_return_type;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-TYP-001", name: "MixedSignComparison", category: "Types", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Comparing signed and unsigned integers causes implicit unsigned coercion of the signed operand. Negative values evaluate as huge positive numbers, causing comparisons to silently invert logic.", check: Check::Syntax(mixed_sign_comparison::mixed_sign_comparison),
        },
        Rule { id: "SV-TYP-008", name: "RedundantCastToSameType", category: "Types", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags dynamic '$cast' calls or static type casts where the source and destination already have identical static types.", check: Check::Syntax(redundant_cast_to_same_type::redundant_cast),
        },
        Rule { id: "SV-TYP-009", name: "ArrayLiteralSizeMismatch", category: "Types", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags positional array assignment patterns '{...} where the number of elements does not match the target fixed array dimension.", check: Check::Syntax(array_literal_size_mismatch::array_literal_size),
        },
        Rule { id: "SV-TYP-010", name: "TaskWithNonVoidReturnType", category: "Types", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows declaring tasks with explicit non-void return types.", check: Check::Syntax(task_with_non_void_return_type::task_return_type),
        },
    ]
}
