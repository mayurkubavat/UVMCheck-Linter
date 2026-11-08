//! The deadcode rules: the Rust twins of `cpp/src/rules/deadcode/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

mod accidental_empty_conditional;
mod consecutive_dead_store;
mod empty_always_or_initial;
mod empty_fork_join;
mod empty_task_or_function;
mod unreachable_statements_post_return;
mod unread_class_property;
mod unreferenced_parameter;
mod unused_generate_block;
mod unused_local_method;
mod unused_pure_routine_result;
mod unused_routine_argument;
mod unused_variable;
mod unwritten_class_property;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-DED-001", name: "UnusedVariable", check: Check::Syntax(unused_variable::unused_variable) },
        Rule { id: "SV-DED-002", name: "UnreachableStatementsPostReturn", check: Check::Syntax(unreachable_statements_post_return::unreachable_after_exit) },
        Rule { id: "SV-DED-003", name: "AccidentalEmptyConditional", check: Check::Syntax(accidental_empty_conditional::empty_conditional) },
        Rule { id: "SV-DED-004", name: "ConsecutiveDeadStore", check: Check::Syntax(consecutive_dead_store::dead_store) },
        Rule { id: "SV-DED-005", name: "UnreadClassProperty", check: Check::Syntax(unread_class_property::unread_property) },
        Rule { id: "SV-DED-006", name: "UnwrittenClassProperty", check: Check::Syntax(unwritten_class_property::unwritten_property) },
        Rule { id: "SV-DED-007", name: "UnusedPureRoutineResult", check: Check::Syntax(unused_pure_routine_result::unused_pure_result) },
        Rule { id: "SV-DED-008", name: "UnusedGenerateBlock", check: Check::Syntax(unused_generate_block::false_condition) },
        Rule { id: "SV-DED-009", name: "EmptyAlwaysOrInitial", check: Check::Syntax(empty_always_or_initial::empty_procedural_block) },
        Rule { id: "SV-DED-010", name: "EmptyForkJoin", check: Check::Syntax(empty_fork_join::empty_fork_join) },
        Rule { id: "SV-DED-011", name: "EmptyTaskOrFunction", check: Check::Syntax(empty_task_or_function::empty_routine) },
        Rule { id: "SV-DED-012", name: "UnreferencedParameter", check: Check::Syntax(unreferenced_parameter::unreferenced_parameter) },
        Rule { id: "SV-DED-013", name: "UnusedRoutineArgument", check: Check::Syntax(unused_routine_argument::unused_routine_argument) },
        Rule { id: "SV-DED-014", name: "UnusedLocalMethod", check: Check::Syntax(unused_local_method::unused_local_method) },
    ]
}
