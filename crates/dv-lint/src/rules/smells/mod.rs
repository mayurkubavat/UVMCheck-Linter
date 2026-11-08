//! The smells rules: the Rust twins of `cpp/src/rules/smells/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

mod cyclomatic_complexity_god_method;
mod generic_report_id;
mod magic_delay_in_sequence;
mod scratchpad_class_property;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule {
            id: "SV-SML-001",
            name: "ScratchpadClassProperty",
            check: Check::Syntax(scratchpad_class_property::scratchpad_property),
        },
        Rule {
            id: "SV-SML-002",
            name: "MagicDelayInSequence",
            check: Check::Syntax(magic_delay_in_sequence::magic_delay_in_sequence),
        },
        Rule { id: "SV-SML-003", name: "GenericReportId", check: Check::Syntax(generic_report_id::generic_report_id) },
        Rule {
            id: "SV-SML-004",
            name: "CyclomaticComplexityGodMethod",
            check: Check::Syntax(cyclomatic_complexity_god_method::god_method),
        },
    ]
}
