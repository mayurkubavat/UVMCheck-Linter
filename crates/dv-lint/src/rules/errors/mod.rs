//! The errors rules: the Rust twins of `cpp/src/rules/errors/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

mod blanket_error_demotion;
mod checker_cannot_fail;
mod failure_not_reported;
mod fixed_delay_before_drop;
mod process_termination_in_class;
mod use_after_failed_null_check;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-ERR-001", name: "FailureNotReported", check: Check::Syntax(failure_not_reported::failure_not_reported) },
        Rule { id: "SV-ERR-002", name: "UseAfterFailedNullCheck", check: Check::Syntax(use_after_failed_null_check::use_after_failed_null_check) },
        Rule { id: "SV-ERR-003", name: "ProcessTerminationInClass", check: Check::Syntax(process_termination_in_class::termination_in_class) },
        Rule { id: "UVM-REP-006", name: "BlanketErrorDemotion", check: Check::Mixed(blanket_error_demotion::blanket_error_demotion) },
        Rule { id: "UVM-TST-001", name: "CheckerCannotFail", check: Check::Mixed(checker_cannot_fail::checker_cannot_fail) },
        Rule { id: "UVM-TST-002", name: "FixedDelayBeforeDrop", check: Check::Syntax(fixed_delay_before_drop::fixed_delay_before_drop) },
    ]
}
