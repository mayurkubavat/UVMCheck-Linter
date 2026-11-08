//! The performance rules: the Rust twins of `cpp/src/rules/performance/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

mod array_locator_empty_queue_deref;
mod array_reduction_width_truncation;
mod associative_array_mutating_read;
mod class_instantiation_in_loop;
mod dyn_array_incremental_resize_in_loop;
mod dynamic_array_new_in_loop;
mod fork_in_clocked_loop;
mod nba_on_class_property;
mod pass_array_by_value;
mod procedural_zero_delay;
mod queue_concat_append;
mod queue_slice_pop_first;
mod string_concat_in_loop;
mod string_key_associative_array;
mod unbounded_dynamic_array_in_constraint;
mod unshielded_verbosity_format_eval;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-ASC-001", name: "AssociativeArrayMutatingRead", check: Check::Syntax(associative_array_mutating_read::assoc_mutating_read) },
        Rule { id: "SV-LOC-001", name: "ArrayLocatorEmptyQueueDeref", check: Check::Syntax(array_locator_empty_queue_deref::locator_deref) },
        Rule { id: "SV-OOP-010", name: "ClassInstantiationInLoop", check: Check::Syntax(class_instantiation_in_loop::new_in_loop) },
        Rule { id: "SV-PRF-001", name: "PassArrayByValue", check: Check::Model(pass_array_by_value::pass_array_by_value) },
        Rule { id: "SV-PRF-002", name: "ArrayReductionWidthTruncation", check: Check::Syntax(array_reduction_width_truncation::array_reduction_width) },
        Rule { id: "SV-PRF-003", name: "DynArrayIncrementalResizeInLoop", check: Check::Mixed(dyn_array_incremental_resize_in_loop::ast_resize_in_loop) },
        Rule { id: "SV-PRF-004", name: "ProceduralZeroDelay", check: Check::Mixed(procedural_zero_delay::ast_zero_delay) },
        Rule { id: "SV-PRF-005", name: "UnshieldedVerbosityFormatEval", check: Check::Mixed(unshielded_verbosity_format_eval::ast_unshielded_format) },
        Rule { id: "SV-PRF-006", name: "StringKeyAssociativeArray", check: Check::Syntax(string_key_associative_array::string_key_assoc) },
        Rule { id: "SV-PRF-007", name: "QueueConcatAppend", check: Check::Syntax(queue_concat_append::queue_concat_append) },
        Rule { id: "SV-PRF-008", name: "QueueSlicePopFirst", check: Check::Mixed(queue_slice_pop_first::ast_queue_slice) },
        Rule { id: "SV-PRF-009", name: "NbaOnClassProperty", check: Check::Mixed(nba_on_class_property::ast_nba_on_property) },
        Rule { id: "SV-PRF-010", name: "StringConcatInLoop", check: Check::Syntax(string_concat_in_loop::concat_in_loop) },
        Rule { id: "SV-PRF-011", name: "DynamicArrayNewInLoop", check: Check::Syntax(dynamic_array_new_in_loop::new_array_in_loop) },
        Rule { id: "SV-PRF-012", name: "UnboundedDynamicArrayInConstraint", check: Check::Syntax(unbounded_dynamic_array_in_constraint::unbounded_size) },
        Rule { id: "SV-PRF-013", name: "ForkInClockedLoop", check: Check::Syntax(fork_in_clocked_loop::fork_in_clocked_loop) },
    ]
}
