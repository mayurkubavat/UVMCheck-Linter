//! The correctness rules: the Rust twins of `cpp/src/rules/correctness/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

mod assignment_in_expression;
mod bit_select_out_of_bounds;
mod case_without_default;
mod casex_prohibited;
mod equality_with_xz;
mod identical_branches;
mod integer_division_to_real;
mod intra_assignment_delay;
mod mismatched_field_in_copy_compare;
mod missing_timescale_in_module;
mod modifying_loop_index_inside_for;
mod non_blocking_in_function;
mod non_exhaustive_enum_case;
mod non_short_circuit_logical;
mod off_by_one_queue_index;
mod part_select_reversed;
mod procedural_assignment_as_sub_expression;
mod real_equality;
mod repeated_else_if_condition;
mod self_assignment;
mod self_comparison;
mod streaming_concatenation_size_mismatch;
mod suspicious_argument_order;
mod tautological_check_compare;
mod timescale_precision_mismatch;
mod unsized_literal_in_concat;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-ARR-001", name: "OffByOneQueueIndex", check: Check::Syntax(off_by_one_queue_index::off_by_one_index) },
        Rule { id: "SV-BUG-001", name: "IdenticalBranches", check: Check::Syntax(identical_branches::identical_branches) },
        Rule { id: "SV-BUG-002", name: "RepeatedElseIfCondition", check: Check::Syntax(repeated_else_if_condition::repeated_else_if) },
        Rule { id: "SV-BUG-003", name: "SelfAssignment", check: Check::Syntax(self_assignment::self_assignment) },
        Rule { id: "SV-BUG-004", name: "MismatchedFieldInCopyCompare", check: Check::Syntax(mismatched_field_in_copy_compare::mismatched_field) },
        Rule { id: "SV-BUG-005", name: "IntegerDivisionToReal", check: Check::Syntax(integer_division_to_real::integer_division_to_real) },
        Rule { id: "SV-BUG-006", name: "RealEquality", check: Check::Syntax(real_equality::real_equality) },
        Rule { id: "SV-BUG-007", name: "SuspiciousArgumentOrder", check: Check::Syntax(suspicious_argument_order::suspicious_argument_order) },
        Rule { id: "SV-COR-001", name: "TautologicalCheckCompare", check: Check::Syntax(tautological_check_compare::tautological_compare) },
        Rule { id: "SV-COR-005", name: "NonShortCircuitLogical", check: Check::Syntax(non_short_circuit_logical::non_short_circuit) },
        Rule { id: "SV-COR-006", name: "CaseWithoutDefault", check: Check::Syntax(case_without_default::case_without_default) },
        Rule { id: "SV-COR-007", name: "AssignmentInExpression", check: Check::Syntax(assignment_in_expression::assignment_in_condition) },
        Rule { id: "SV-COR-008", name: "SelfComparison", check: Check::Syntax(self_comparison::self_comparison) },
        Rule { id: "SV-COR-009", name: "CasexProhibited", check: Check::Syntax(casex_prohibited::casex_prohibited) },
        Rule { id: "SV-COR-011", name: "ModifyingLoopIndexInsideFor", check: Check::Syntax(modifying_loop_index_inside_for::loop_index_modified) },
        Rule { id: "SV-COR-020", name: "ProceduralAssignmentAsSubExpression", check: Check::Syntax(procedural_assignment_as_sub_expression::assignment_sub_expr) },
        Rule { id: "SV-DLY-002", name: "IntraAssignmentDelay", check: Check::Syntax(intra_assignment_delay::intra_assignment_delay) },
        Rule { id: "SV-FSM-001", name: "NonExhaustiveEnumCase", check: Check::Syntax(non_exhaustive_enum_case::non_exhaustive_enum_case) },
        Rule { id: "SV-LANG-001", name: "EqualityWithXz", check: Check::Syntax(equality_with_xz::equality_with_xz) },
        Rule { id: "SV-LANG-002", name: "BitSelectOutOfBounds", check: Check::Syntax(bit_select_out_of_bounds::bit_select_out_of_bounds) },
        Rule { id: "SV-LANG-003", name: "UnsizedLiteralInConcat", check: Check::Syntax(unsized_literal_in_concat::unsized_in_concat) },
        Rule { id: "SV-LANG-004", name: "PartSelectReversed", check: Check::Syntax(part_select_reversed::part_select_reversed) },
        Rule { id: "SV-LANG-005", name: "NonBlockingInFunction", check: Check::Syntax(non_blocking_in_function::nonblocking_in_function) },
        Rule { id: "SV-STR-001", name: "StreamingConcatenationSizeMismatch", check: Check::Syntax(streaming_concatenation_size_mismatch::stream_slice) },
        Rule { id: "SV-TIM-001", name: "TimescalePrecisionMismatch", check: Check::Syntax(timescale_precision_mismatch::timescale_precision) },
        Rule { id: "SV-TIM-002", name: "MissingTimescaleInModule", check: Check::Syntax(missing_timescale_in_module::missing_timescale) },
    ]
}
