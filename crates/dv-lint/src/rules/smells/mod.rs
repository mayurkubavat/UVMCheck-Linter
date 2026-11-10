//! The smells rules: the Rust twins of `cpp/src/rules/smells/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::profile::*;
use crate::{Check, Rule, Severity};

mod cyclomatic_complexity_god_method;
mod generic_report_id;
mod magic_delay_in_sequence;
mod scratchpad_class_property;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-SML-001", name: "ScratchpadClassProperty", category: "Maintainability", severity: Severity::Warning, profiles: SMELLS, guidelines: &[], experimental: false, native: false, about: "Disallow declaring scratchpad variables or loop induction counters as class properties. Move declarations to method stack frames to ensure thread safety and re-entrancy.", check: Check::Syntax(scratchpad_class_property::scratchpad_property),
        },
        Rule { id: "SV-SML-002", name: "MagicDelayInSequence", category: "Maintainability", severity: Severity::Warning, profiles: SMELLS, guidelines: &[], experimental: false, native: false, about: "Disallow hardcoded physical delays (#delay) inside uvm_sequence. Sequences must be clocking-block and frequency agnostic.", check: Check::Syntax(magic_delay_in_sequence::magic_delay_in_sequence),
        },
        Rule { id: "SV-SML-003", name: "GenericReportId", category: "Maintainability", severity: Severity::Info, profiles: SMELLS, guidelines: &[], experimental: false, native: false, about: "Disallow generic single-word report IDs ('ERR', 'INFO', 'DEBUG') in UVM reporting macros. Use feature-scoped hierarchical identifiers to support precise log filtering.", check: Check::Syntax(generic_report_id::generic_report_id) },
        Rule { id: "SV-SML-004", name: "CyclomaticComplexityGodMethod", category: "Maintainability", severity: Severity::Warning, profiles: SMELLS, guidelines: &["MG-9.1"], experimental: false, native: false, about: "Detects subroutines with high McCabe Cyclomatic Complexity ($M > 15$). Complex control flow obscures edge cases and inhibits functional verification.", check: Check::Syntax(cyclomatic_complexity_god_method::god_method),
        },
    ]
}
