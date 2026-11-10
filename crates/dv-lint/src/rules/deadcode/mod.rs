//! The deadcode rules: the Rust twins of `cpp/src/rules/deadcode/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::profile::*;
use crate::{Check, Rule, Severity};

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
        Rule { id: "SV-DED-001", name: "UnusedVariable", category: "DeadCode", severity: Severity::Info, profiles: SOUNDNESS, guidelines: &["MG-9.1"], experimental: false, native: false, about: "Local variables declared in tasks or functions should be referenced. Unused variables clutter code, consume memory, and often indicate incomplete logic.", check: Check::Syntax(unused_variable::unused_variable) },
        Rule { id: "SV-DED-002", name: "UnreachableStatementsPostReturn", category: "DeadCode", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Detects unreachable statements appearing after an unconditional control-flow termination (return, break, continue). Dead code clutters verification environments and hides logic errors.", check: Check::Syntax(unreachable_statements_post_return::unreachable_after_exit),
        },
        Rule { id: "SV-DED-003", name: "AccidentalEmptyConditional", category: "DeadCode", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Detects accidental semicolons immediately following if() or while() headers. An empty conditional branch causes subsequent blocks to execute unconditionally, introducing severe logic bugs.", check: Check::Syntax(accidental_empty_conditional::empty_conditional),
        },
        Rule { id: "SV-DED-004", name: "ConsecutiveDeadStore", category: "DeadCode", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Detects consecutive assignments to the same variable where the initial value is immediately overwritten without being read. Dead stores waste CPU cycles and frequently reveal copy-paste logic bugs.", check: Check::Syntax(consecutive_dead_store::dead_store),
        },
        Rule { id: "SV-DED-005", name: "UnreadClassProperty", category: "DeadCode", severity: Severity::Info, profiles: SOUNDNESS, guidelines: &["MG-9.1"], experimental: false, native: false, about: "Flags class properties that are assigned or written to, but never read anywhere in the class. Unread properties indicate dead instrumentation or abandoned design state.", check: Check::Syntax(unread_class_property::unread_property),
        },
        Rule { id: "SV-DED-006", name: "UnwrittenClassProperty", category: "DeadCode", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &["MG-9.1"], experimental: false, native: false, about: "Flags non-rand class properties that lack an initializer, are read in methods, but never written or assigned. Unwritten properties always evaluate to uninitialized defaults, risking dead branches and silent logic bugs.", check: Check::Syntax(unwritten_class_property::unwritten_property),
        },
        Rule { id: "SV-DED-007", name: "UnusedPureRoutineResult", category: "DeadCode", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags calls to pure functions or value-returning routines ($sformatf, $realtime, len, toupper, sum) used as standalone statements where the result is discarded.", check: Check::Syntax(unused_pure_routine_result::unused_pure_result),
        },
        Rule { id: "SV-DED-008", name: "UnusedGenerateBlock", category: "DeadCode", severity: Severity::Info, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags conditional statements or generate blocks with constant false conditions ('0') and no 'else' clause.", check: Check::Syntax(unused_generate_block::false_condition),
        },
        Rule { id: "SV-DED-009", name: "EmptyAlwaysOrInitial", category: "DeadCode", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows initial, always, always_comb, always_ff, always_latch, or final procedural blocks with empty bodies.", check: Check::Syntax(empty_always_or_initial::empty_procedural_block),
        },
        Rule { id: "SV-DED-010", name: "EmptyForkJoin", category: "DeadCode", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows empty fork...join, fork...join_any, or fork...join_none blocks that contain zero statements.", check: Check::Syntax(empty_fork_join::empty_fork_join) },
        Rule { id: "SV-DED-011", name: "EmptyTaskOrFunction", category: "DeadCode", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &["MG-9.1"], experimental: false, native: false, about: "Flags concrete, non-virtual, non-extern tasks and functions with empty bodies (zero statements). Empty routines cause useless call overhead and indicate incomplete or dead logic.", check: Check::Syntax(empty_task_or_function::empty_routine),
        },
        Rule { id: "SV-DED-012", name: "UnreferencedParameter", category: "DeadCode", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &["MG-9.1"], experimental: false, native: false, about: "Flags parameter and localparam declarations that are never referenced within their scope or testbench. Unused parameters create clutter and indicate dead or abandoned configurations.", check: Check::Syntax(unreferenced_parameter::unreferenced_parameter),
        },
        Rule { id: "SV-DED-013", name: "UnusedRoutineArgument", category: "Maintainability", severity: Severity::Info, profiles: SMELLS, guidelines: &["MG-9.1"], experimental: false, native: false, about: "Flags task and function arguments that the body never uses (clang-tidy misc-unused-parameters, Pylint W0613). Virtual methods and overrides, phase methods, DPI routines and empty bodies are skipped.", check: Check::Syntax(unused_routine_argument::unused_routine_argument),
        },
        Rule { id: "SV-DED-014", name: "UnusedLocalMethod", category: "Maintainability", severity: Severity::Warning, profiles: SMELLS, guidelines: &["MG-9.1"], experimental: false, native: false, about: "Flags a local task or function that no method of its class calls. A local method is invisible outside the class, so with no caller it is dead.", check: Check::Syntax(unused_local_method::unused_local_method),
        },
    ]
}
