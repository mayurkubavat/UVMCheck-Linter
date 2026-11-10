//! The errors rules: the Rust twins of `cpp/src/rules/errors/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::profile::*;
use crate::{Check, Rule, Severity};

mod blanket_error_demotion;
mod checker_cannot_fail;
mod failure_not_reported;
mod fixed_delay_before_drop;
mod process_termination_in_class;
mod use_after_failed_null_check;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-ERR-001", name: "FailureNotReported", category: "Correctness", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &["MG-8.1"], experimental: false, native: false, about: "Flags randomize(), $cast(), uvm_config_db::get(), try_* and compare() status checks whose failure branch is empty or only reports below error severity (Sonar S108/S1166).", check: Check::Syntax(failure_not_reported::failure_not_reported),
        },
        Rule { id: "SV-ERR-002", name: "UseAfterFailedNullCheck", category: "Correctness", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &["MG-8.1"], experimental: false, native: false, about: "Flags a handle that a null check, a failed uvm_config_db::get() or a failed $cast() showed to be null, dereferenced later on the same path because the failure branch did not stop (SpotBugs NP_NULL_ON_SOME_PATH).", check: Check::Syntax(use_after_failed_null_check::use_after_failed_null_check),
        },
        Rule { id: "SV-ERR-003", name: "ProcessTerminationInClass", category: "Correctness", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &["MG-8.1"], experimental: false, native: false, about: "Flags $finish, $stop and $exit called from class methods. The run then skips the extract, check and report phases and the UVM report summary (Sonar S1147).", check: Check::Syntax(process_termination_in_class::termination_in_class),
        },
        Rule { id: "UVM-REP-006", name: "BlanketErrorDemotion", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &["MG-8.1"], experimental: false, native: false, about: "Flags report catchers that demote or swallow errors without filtering on ID or message, and severity overrides/actions that disable UVM_ERROR or UVM_FATAL for every ID (Pylint W0718 broad-exception-caught).", check: Check::Mixed(blanket_error_demotion::blanket_error_demotion),
        },
        Rule { id: "UVM-TST-001", name: "CheckerCannotFail", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &["MG-8.1"], experimental: false, native: false, about: "Flags scoreboard and checker classes with no way to report a failure: no `uvm_error or `uvm_fatal, no assertion and no comparator, in the class, its bases or the helpers it calls (Sonar S2699, tests without assertions).", check: Check::Mixed(checker_cannot_fail::checker_cannot_fail),
        },
        Rule { id: "UVM-TST-002", name: "FixedDelayBeforeDrop", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Flags a constant #delay immediately before drop_objection(), used to let traffic drain (xUnit 'sleepy test', Sonar S2925).", check: Check::Syntax(fixed_delay_before_drop::fixed_delay_before_drop),
        },
    ]
}
