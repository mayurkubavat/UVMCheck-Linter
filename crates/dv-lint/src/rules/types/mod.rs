//! The types rules: the Rust twins of `cpp/src/rules/types/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

mod array_literal_size_mismatch;
mod mixed_sign_comparison;
mod redundant_cast_to_same_type;
mod task_with_non_void_return_type;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-TYP-001", name: "MixedSignComparison", check: Check::Syntax(mixed_sign_comparison::mixed_sign_comparison) },
        Rule { id: "SV-TYP-008", name: "RedundantCastToSameType", check: Check::Syntax(redundant_cast_to_same_type::redundant_cast) },
        Rule { id: "SV-TYP-009", name: "ArrayLiteralSizeMismatch", check: Check::Syntax(array_literal_size_mismatch::array_literal_size) },
        Rule { id: "SV-TYP-010", name: "TaskWithNonVoidReturnType", check: Check::Syntax(task_with_non_void_return_type::task_return_type) },
    ]
}
