//! The metrics rules: the Rust twins of `cpp/src/rules/metrics/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

mod cognitive_complexity;
mod complex_condition;
mod deep_nesting;
mod large_class;
mod long_routine;
mod long_task;
mod rand_field_budget;
mod too_many_arguments;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule {
            id: "SV-MET-001",
            name: "CognitiveComplexity",
            check: Check::Syntax(cognitive_complexity::cognitive_complexity),
        },
        Rule { id: "SV-MET-002", name: "LongRoutine", check: Check::Syntax(long_routine::long_routine) },
        Rule {
            id: "SV-MET-003",
            name: "TooManyArguments",
            check: Check::Syntax(too_many_arguments::too_many_arguments),
        },
        Rule { id: "SV-MET-004", name: "DeepNesting", check: Check::Syntax(deep_nesting::deep_nesting) },
        Rule { id: "SV-MET-005", name: "ComplexCondition", check: Check::Syntax(complex_condition::complex_condition) },
        Rule { id: "SV-MET-006", name: "LargeClass", check: Check::Syntax(large_class::large_class) },
        Rule { id: "SV-MET-007", name: "LongTask", check: Check::Syntax(long_task::long_task) },
        Rule { id: "SV-MET-008", name: "RandFieldBudget", check: Check::Syntax(rand_field_budget::rand_field_budget) },
    ]
}
