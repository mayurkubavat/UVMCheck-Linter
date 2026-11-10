//! The metrics rules: the Rust twins of `cpp/src/rules/metrics/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::profile::*;
use crate::{Check, Rule, Severity};

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
        Rule { id: "SV-MET-001", name: "CognitiveComplexity", category: "Maintainability", severity: Severity::Warning, profiles: SMELLS, guidelines: &["MG-9.1"], experimental: false, native: false, about: "Flags routines whose Sonar cognitive complexity (nesting-weighted branching, with fork/join counted as nesting) exceeds the threshold (default 25; Sonar S3776).", check: Check::Syntax(cognitive_complexity::cognitive_complexity),
        },
        Rule { id: "SV-MET-002", name: "LongRoutine", category: "Maintainability", severity: Severity::Warning, profiles: SMELLS, guidelines: &["MG-9.1"], experimental: false, native: false, about: "Flags routines with more statements than the threshold (default 100; clang-tidy readability-function-size, Checkstyle MethodLength).", check: Check::Syntax(long_routine::long_routine) },
        Rule { id: "SV-MET-003", name: "TooManyArguments", category: "Maintainability", severity: Severity::Warning, profiles: SMELLS, guidelines: &["MG-9.1"], experimental: false, native: false, about: "Flags tasks and functions with more formal arguments than the threshold (default 7; Pylint R0913, Sonar S107). Overrides are judged at the base declaration only.", check: Check::Syntax(too_many_arguments::too_many_arguments),
        },
        Rule { id: "SV-MET-004", name: "DeepNesting", category: "Maintainability", severity: Severity::Warning, profiles: SMELLS, guidelines: &["MG-9.1"], experimental: false, native: false, about: "Flags routines whose control structures nest deeper than the threshold (default 5; Checkstyle NestedIfDepth, clang-tidy readability-function-size NestingThreshold).", check: Check::Syntax(deep_nesting::deep_nesting) },
        Rule { id: "SV-MET-005", name: "ComplexCondition", category: "Maintainability", severity: Severity::Warning, profiles: SMELLS, guidelines: &["MG-9.1"], experimental: false, native: false, about: "Flags conditions with more && / || operators than the threshold (default 5; Pylint R0916, Checkstyle BooleanExpressionComplexity).", check: Check::Syntax(complex_condition::complex_condition) },
        Rule { id: "SV-MET-006", name: "LargeClass", category: "Maintainability", severity: Severity::Info, profiles: SMELLS, guidelines: &["MG-3.1"], experimental: false, native: false, about: "Flags classes with more than 30 properties or 30 methods; config objects and sequence items may have 80 properties, register models are exempt (PMD TooManyFields and TooManyMethods, Pylint R0902/R0904).", check: Check::Syntax(large_class::large_class) },
        Rule { id: "SV-MET-007", name: "LongTask", category: "Maintainability", severity: Severity::Warning, profiles: SMELLS, guidelines: &["MG-9.1"], experimental: false, native: false, about: "Flags tasks longer than the line budget (default 40 source lines, option max_lines). A long task is hard to review and to cover; split it into named tasks with one job each.", check: Check::Syntax(long_task::long_task) },
        Rule { id: "SV-MET-008", name: "RandFieldBudget", category: "Maintainability", severity: Severity::Warning, profiles: SMELLS, guidelines: &["MG-9.1"], experimental: false, native: false, about: "Flags a sequence item that declares more than the rand-field budget (default 8, option max_rand), counting rand and randc properties of the class itself. A wide item is slow to solve and hard to constrain; split the fields across items.", check: Check::Syntax(rand_field_budget::rand_field_budget) },
    ]
}
