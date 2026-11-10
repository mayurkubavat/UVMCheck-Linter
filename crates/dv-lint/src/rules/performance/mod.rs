//! The performance rules: the Rust twins of `cpp/src/rules/performance/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::profile::*;
use crate::{Check, Rule, Severity};

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
        Rule { id: "SV-ASC-001", name: "AssociativeArrayMutatingRead", category: "Performance", severity: Severity::Warning, profiles: PERFORMANCE, guidelines: &[], experimental: false, native: false, about: "Detects direct read lookups on associative arrays in expressions without checking exists(). In SystemVerilog (IEEE 1800 § 7.8.1), reading a missing key silently allocates and inserts it with a default value.", check: Check::Syntax(associative_array_mutating_read::assoc_mutating_read),
        },
        Rule { id: "SV-LOC-001", name: "ArrayLocatorEmptyQueueDeref", category: "Performance", severity: Severity::Error, profiles: PERFORMANCE, guidelines: &[], experimental: false, native: false, about: "Disallows immediate element dereferencing on array locator methods (e.g., q.find()[0]). Locator methods return an unpacked queue that may be empty, causing out-of-bounds runtime errors (IEEE 1800 § 7.12.1).", check: Check::Syntax(array_locator_empty_queue_deref::locator_deref),
        },
        Rule { id: "SV-OOP-010", name: "ClassInstantiationInLoop", category: "Performance", severity: Severity::Warning, profiles: PERFORMANCE, guidelines: &[], experimental: false, native: false, about: "Disallows direct class instantiation via new() inside simulation loops to prevent heap contention and memory allocation overhead.", check: Check::Syntax(class_instantiation_in_loop::new_in_loop),
        },
        Rule { id: "SV-PRF-001", name: "PassArrayByValue", category: "Performance", severity: Severity::Warning, profiles: PERFORMANCE, guidelines: &[], experimental: false, native: false, about: "Passing dynamic arrays, queues, associative arrays, unpacked arrays, or unpacked structs by value (input) causes expensive memory duplication on every call. Use 'const ref' or 'ref' instead.", check: Check::Model(pass_array_by_value::pass_array_by_value),
        },
        Rule { id: "SV-PRF-002", name: "ArrayReductionWidthTruncation", category: "Performance", severity: Severity::Error, profiles: PERFORMANCE, guidelines: &[], experimental: false, native: false, about: "Array reduction methods (.sum, .product) accumulate at element bit-width before extension to LHS. Summing sub-32-bit elements into 32/64-bit integers causes silent wraparound unless cast in with().", check: Check::Syntax(array_reduction_width_truncation::array_reduction_width),
        },
        Rule { id: "SV-PRF-003", name: "DynArrayIncrementalResizeInLoop", category: "Performance", severity: Severity::Warning, profiles: PERFORMANCE, guidelines: &[], experimental: false, native: false, about: "Growing dynamic arrays incrementally inside loops with new[arr.size()+1](arr) causes O(N^2) memory reallocation bottlenecks. Pre-allocate with new[N] or use SystemVerilog queues instead.", check: Check::Mixed(dyn_array_incremental_resize_in_loop::ast_resize_in_loop),
        },
        Rule { id: "SV-PRF-004", name: "ProceduralZeroDelay", category: "Performance", severity: Severity::Warning, profiles: PERFORMANCE, guidelines: &[], experimental: false, native: false, about: "Procedural delay #0 reschedules execution into the Inactive region. Repeated #0 rescheduling causes delta-cycle thrashing and degrades simulation throughput.", check: Check::Mixed(procedural_zero_delay::ast_zero_delay),
        },
        Rule { id: "SV-PRF-005", name: "UnshieldedVerbosityFormatEval", category: "Performance", severity: Severity::Warning, profiles: PERFORMANCE, guidelines: &[], experimental: false, native: false, about: "Calling $sformatf inside uvm_info/warning without checking uvm_report_enabled formats strings even when messages are discarded by verbosity.", check: Check::Mixed(unshielded_verbosity_format_eval::ast_unshielded_format),
        },
        Rule { id: "SV-PRF-006", name: "StringKeyAssociativeArray", category: "Performance", severity: Severity::Warning, profiles: PERFORMANCE, guidelines: &[], experimental: false, native: false, about: "Detects associative arrays indexed by 'string'. String keys incur hash calculation and dynamic string comparisons on every lookup and insert, degrading simulation throughput.", check: Check::Syntax(string_key_associative_array::string_key_assoc),
        },
        Rule { id: "SV-PRF-007", name: "QueueConcatAppend", category: "Performance", severity: Severity::Warning, profiles: PERFORMANCE, guidelines: &[], experimental: false, native: false, about: "Disallow queue concatenation ('q = {q, x}' or 'q = {x, q}'). Concatenation reallocates memory on every append ($O(N^2)$ in loops); use push_back() or push_front() for amortized $O(1)$ performance.", check: Check::Syntax(queue_concat_append::queue_concat_append),
        },
        Rule { id: "SV-PRF-008", name: "QueueSlicePopFirst", category: "Performance", severity: Severity::Warning, profiles: PERFORMANCE, guidelines: &[], experimental: false, native: false, about: "Disallow queue slicing ('q = q[1:$]') to drop head elements. Queue slicing duplicates all remaining elements into new memory; use pop_front() for in-place $O(1)$ head-pointer advance.", check: Check::Mixed(queue_slice_pop_first::ast_queue_slice),
        },
        Rule { id: "SV-PRF-009", name: "NbaOnClassProperty", category: "Performance", severity: Severity::Warning, profiles: PERFORMANCE, guidelines: &[], experimental: false, native: false, about: "Disallow non-blocking assignments ('<=') on class properties. For heap-allocated class instances, NBAs force the simulator kernel to create dynamic event descriptors in the NBA region of the event wheel. Use blocking assignment ('=').", check: Check::Mixed(nba_on_class_property::ast_nba_on_property),
        },
        Rule { id: "SV-PRF-010", name: "StringConcatInLoop", category: "Performance", severity: Severity::Warning, profiles: PERFORMANCE, guidelines: &[], experimental: false, native: false, about: "Disallows iterative in-place string concatenation or $sformatf recursion inside loop bodies to prevent O(N^2) memory reallocation overhead.", check: Check::Syntax(string_concat_in_loop::concat_in_loop),
        },
        Rule { id: "SV-PRF-011", name: "DynamicArrayNewInLoop", category: "Performance", severity: Severity::Warning, profiles: PERFORMANCE, guidelines: &[], experimental: false, native: false, about: "Disallows dynamic array allocation (new[...]) inside simulation loops to prevent heap contention and memory churn.", check: Check::Syntax(dynamic_array_new_in_loop::new_array_in_loop),
        },
        Rule { id: "SV-PRF-012", name: "UnboundedDynamicArrayInConstraint", category: "Performance", severity: Severity::Warning, profiles: PERFORMANCE, guidelines: &[], experimental: false, native: false, about: "Flags dynamic array or queue size constraints with an upper bound (< or <=) lacking an explicit lower bound.", check: Check::Syntax(unbounded_dynamic_array_in_constraint::unbounded_size),
        },
        Rule { id: "SV-PRF-013", name: "ForkInClockedLoop", category: "Performance", severity: Severity::Warning, profiles: PERFORMANCE, guidelines: &[], experimental: false, native: false, about: "Flags fork/join, join_any, and join_none inside a clocked loop (always_ff, always @(posedge/negedge), or a loop that waits on a clock edge). A thread started on every clock piles work onto the scheduler.", check: Check::Syntax(fork_in_clocked_loop::fork_in_clocked_loop),
        },
    ]
}
