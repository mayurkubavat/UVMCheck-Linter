//! The portability rules: the Rust twins of `cpp/src/rules/portability/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::profile::*;
use crate::{Check, Rule, Severity};

mod global_unit_pollution;
mod hardcoded_host_paths;
mod header_macro_leak;
mod proprietary_vendor_calls;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "PORT-PRP-001", name: "ProprietaryVendorCalls", category: "Portability", severity: Severity::Warning, profiles: SOUNDNESS | SECURITY, guidelines: &["MG-4.3"], experimental: false, native: false, about: "Disallow unguarded simulator-proprietary system tasks ($vcdpluson, $fsdbDumpvars, $shm_open). Vendor-specific calls prevent Verification IP from running on other simulators.", check: Check::Syntax(proprietary_vendor_calls::vendor_calls),
        },
        Rule { id: "PORT-PTH-001", name: "HardcodedHostPaths", category: "Portability", severity: Severity::Error, profiles: SOUNDNESS | SECURITY, guidelines: &["MG-4.3"], experimental: false, native: false, about: "Disallow hardcoded absolute filesystem paths (/home/..., C:\\...) in file operations ($readmemh, $fopen). Absolute paths break regression execution in containerized CI/CD pipelines.", check: Check::Mixed(hardcoded_host_paths::ast_hardcoded_paths),
        },
        Rule { id: "SV-MAC-001", name: "HeaderMacroLeak", category: "Portability", severity: Severity::Warning, profiles: METHODOLOGY | STYLE, guidelines: &["MG-1.3"], experimental: false, native: false, about: "Macros defined inside header files (.svh) must be undef'd with `undef before the end of the file unless registered as public API macros. Leaking macros causes silent parameter overrides and compilation collisions.", check: Check::Syntax(header_macro_leak::header_macro_leak) },
        Rule { id: "SV-MOD-001", name: "GlobalUnitPollution", category: "Portability", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &["MG-1.3"], experimental: false, native: false, about: "Disallow variables, nets, typedefs, tasks, and functions declared directly in the $unit compilation unit scope. Declarations outside a package or module cause cross-file namespace pollution and compile-order dependencies in multi-unit compilation.", check: Check::Syntax(global_unit_pollution::unit_pollution),
        },
    ]
}
