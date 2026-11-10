//! The correctness rules: the Rust twins of `cpp/src/rules/correctness/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::profile::*;
use crate::{Check, Rule, Severity};

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
        Rule { id: "SV-ARR-001", name: "OffByOneQueueIndex", category: "Correctness", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Detects out-of-bounds off-by-one queue and dynamic array indexing (e.g. 'q[q.size()]'). SystemVerilog silently returns uninitialized 'x/0 instead of raising an exception.", check: Check::Syntax(off_by_one_queue_index::off_by_one_index),
        },
        Rule { id: "SV-BUG-001", name: "IdenticalBranches", category: "Correctness", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags if/else and case branches whose bodies are token-identical to an earlier branch of the same statement (clang-tidy bugprone-branch-clone, Sonar S1871).", check: Check::Syntax(identical_branches::identical_branches),
        },
        Rule { id: "SV-BUG-002", name: "RepeatedElseIfCondition", category: "Correctness", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags a condition that repeats an earlier condition of the same if/else-if chain, which makes the later branch dead (Sonar S1862).", check: Check::Syntax(repeated_else_if_condition::repeated_else_if),
        },
        Rule { id: "SV-BUG-003", name: "SelfAssignment", category: "Correctness", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags 'x = x' assignments, typically a constructor or setter whose argument shadows the property it was meant to set (clang-tidy bugprone-self-assignment, Sonar S1656).", check: Check::Syntax(self_assignment::self_assignment) },
        Rule { id: "SV-BUG-004", name: "MismatchedFieldInCopyCompare", category: "Correctness", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &["MG-4.4"], experimental: false, native: false, about: "In do_copy/do_compare and similar field-wise methods, flags a field of this object paired with a differently named field of the other object when the matching field exists.", check: Check::Syntax(mismatched_field_in_copy_compare::mismatched_field),
        },
        Rule { id: "SV-BUG-005", name: "IntegerDivisionToReal", category: "Correctness", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags integer divisions whose result is used as a real value, which truncates before the conversion (Sonar S2184, SpotBugs ICAST_IDIV_CAST_TO_DOUBLE).", check: Check::Syntax(integer_division_to_real::integer_division_to_real),
        },
        Rule { id: "SV-BUG-006", name: "RealEquality", category: "Correctness", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags == and != on real or shortreal operands, except against an exact zero (Sonar S1244, clang-tidy float equality).", check: Check::Syntax(real_equality::real_equality) },
        Rule { id: "SV-BUG-007", name: "SuspiciousArgumentOrder", category: "Correctness", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &["MG-2.4"], experimental: false, native: false, about: "Flags positional calls where an argument's name matches a different parameter of the callee, such as write(len, addr) for write(addr, len) (clang-tidy readability-suspicious-call-argument, Sonar S2234).", check: Check::Syntax(suspicious_argument_order::suspicious_argument_order),
        },
        Rule { id: "SV-COR-001", name: "TautologicalCheckCompare", category: "Correctness", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Detects tautological comparison between identical expressions (e.g. 'act_pkt.data == act_pkt.data'). Self-comparisons in scoreboards and checks always evaluate to true, masking hardware bugs.", check: Check::Syntax(tautological_check_compare::tautological_compare),
        },
        Rule { id: "SV-COR-005", name: "NonShortCircuitLogical", category: "Correctness", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows using bitwise AND ('&') or bitwise OR ('|') instead of logical short-circuiting operators ('&&', '||') inside conditional statements.", check: Check::Syntax(non_short_circuit_logical::non_short_circuit),
        },
        Rule { id: "SV-COR-006", name: "CaseWithoutDefault", category: "Correctness", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags 'case', 'casex', and 'casez' statements that lack an explicit 'default:' fallback branch.", check: Check::Syntax(case_without_default::case_without_default),
        },
        Rule { id: "SV-COR-007", name: "AssignmentInExpression", category: "Correctness", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows assignment expressions ('=' or compound assignments) inside conditional predicates or loop conditions.", check: Check::Syntax(assignment_in_expression::assignment_in_condition),
        },
        Rule { id: "SV-COR-008", name: "SelfComparison", category: "Correctness", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows self-comparisons where an operand is compared to itself (e.g. a == a), which tautologically evaluates to constant true or false.", check: Check::Syntax(self_comparison::self_comparison) },
        Rule { id: "SV-COR-009", name: "CasexProhibited", category: "Correctness", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &["MG-6.3"], experimental: false, native: false, about: "Disallows use of 'casex' statements. 'casex' treats unknown simulation 'x' values as wildcards, masking uninitialized logic bugs and timing errors.", check: Check::Syntax(casex_prohibited::casex_prohibited) },
        Rule { id: "SV-COR-011", name: "ModifyingLoopIndexInsideFor", category: "Correctness", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags modifications to the loop control variable inside a 'for' loop body. Mutating the loop index inside the body causes infinite loops, skipped iterations, and indexing errors.", check: Check::Syntax(modifying_loop_index_inside_for::loop_index_modified),
        },
        Rule { id: "SV-COR-020", name: "ProceduralAssignmentAsSubExpression", category: "Correctness", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows embedded assignment expressions used as sub-expressions within conditionals or arithmetic operations.", check: Check::Syntax(procedural_assignment_as_sub_expression::assignment_sub_expr),
        },
        Rule { id: "SV-DLY-002", name: "IntraAssignmentDelay", category: "Correctness", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows intra-assignment timing controls (e.g., a = #5 b; or a <= #5 b;) on procedural assignments. Intra-assignment delays cause simulation race conditions and are non-synthesizable.", check: Check::Syntax(intra_assignment_delay::intra_assignment_delay),
        },
        Rule { id: "SV-FSM-001", name: "NonExhaustiveEnumCase", category: "Correctness", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Case statement switching over an enum type does not explicitly handle all declared enum variants while using a default: arm. Catch-all default arms silently swallow newly added specification states and mask FSM decoder bugs.", check: Check::Syntax(non_exhaustive_enum_case::non_exhaustive_enum_case),
        },
        Rule { id: "SV-LANG-001", name: "EqualityWithXz", category: "Correctness", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Detects equality ('==', '!=') checks against 4-state unknown literals ('x, 'z). Logical equality against unknowns evaluates to 'x (false in branch conditions), silently masking bugs. Use case equality ('===', '!==') instead.", check: Check::Syntax(equality_with_xz::equality_with_xz) },
        Rule { id: "SV-LANG-002", name: "BitSelectOutOfBounds", category: "Correctness", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Detects static out-of-bounds bit or element selects on vectors and arrays with fixed ranges. SystemVerilog silently returns 'x for out-of-bounds accesses without crashing, causing silent data corruption.", check: Check::Syntax(bit_select_out_of_bounds::bit_select_out_of_bounds),
        },
        Rule { id: "SV-LANG-003", name: "UnsizedLiteralInConcat", category: "Correctness", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallow unbased unsized integer literals inside concatenations ({1, 0, data}). Per IEEE 1800 § 11.4.12, unbased constants expand to 32 bits each, causing unintended vector widening.", check: Check::Syntax(unsized_literal_in_concat::unsized_in_concat),
        },
        Rule { id: "SV-LANG-004", name: "PartSelectReversed", category: "Correctness", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Detects reversed or inverted part-select slicing on vectors (e.g. vec[0:7] on logic [7:0] vec). Part-select slicing bounds must match the declared vector endianness.", check: Check::Syntax(part_select_reversed::part_select_reversed),
        },
        Rule { id: "SV-LANG-005", name: "NonBlockingInFunction", category: "Correctness", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Prohibits non-blocking assignments ('<=') to variables inside functions: the update lands after the caller has moved on. Interface and clocking block drives are exempt (IEEE 1800 § 13.4.4 allows non-blocking assignments in functions).", check: Check::Syntax(non_blocking_in_function::nonblocking_in_function),
        },
        Rule { id: "SV-STR-001", name: "StreamingConcatenationSizeMismatch", category: "Correctness", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows streaming concatenation operators ({>> { ... }} / {<< { ... }}) without explicit slice size. Unbounded slice streaming leads to silent truncation and byte alignment bugs per IEEE 1800 § 11.4.14.", check: Check::Syntax(streaming_concatenation_size_mismatch::stream_slice),
        },
        Rule { id: "SV-TIM-001", name: "TimescalePrecisionMismatch", category: "Correctness", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags `timescale directives where the time precision is coarser than the time unit (IEEE 1800 § 3.14.2).", check: Check::Syntax(timescale_precision_mismatch::timescale_precision),
        },
        Rule { id: "SV-TIM-002", name: "MissingTimescaleInModule", category: "Correctness", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags files and modules containing procedural delays (#) that lack an explicit `timescale or timeunit declaration.", check: Check::Syntax(missing_timescale_in_module::missing_timescale),
        },
    ]
}
