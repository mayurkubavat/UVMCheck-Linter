//! The portability rules: the Rust twins of `cpp/src/rules/portability/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

mod global_unit_pollution;
mod hardcoded_host_paths;
mod header_macro_leak;
mod proprietary_vendor_calls;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "PORT-PRP-001", name: "ProprietaryVendorCalls", check: Check::Syntax(proprietary_vendor_calls::vendor_calls) },
        Rule { id: "PORT-PTH-001", name: "HardcodedHostPaths", check: Check::Mixed(hardcoded_host_paths::ast_hardcoded_paths) },
        Rule { id: "SV-MAC-001", name: "HeaderMacroLeak", check: Check::Syntax(header_macro_leak::header_macro_leak) },
        Rule { id: "SV-MOD-001", name: "GlobalUnitPollution", check: Check::Syntax(global_unit_pollution::unit_pollution) },
    ]
}
