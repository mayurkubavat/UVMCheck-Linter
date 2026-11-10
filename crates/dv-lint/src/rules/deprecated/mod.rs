//! The deprecated rules: the Rust twins of `cpp/src/rules/deprecated/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::profile::*;
use crate::{Check, Rule, Severity};

mod defparam_deprecation;
mod legacy_always_block;
mod legacy_reg_wire_in_testbench;
mod non_ansi_port_list;
mod procedural_continuous_assign;
mod program_block_deprecation;
mod random_system_function;
mod uvm_banned_field_macros;
mod uvm_banned_uvm_do_macros;
mod uvm_deprecated_config_calls;
mod uvm_deprecated_printer_knobs;
mod uvm_deprecated_report_server;
mod uvm_deprecated_test_done;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-BAN-001", name: "LegacyRegWireInTestbench", category: "Deprecated", severity: Severity::Info, profiles: METHODOLOGY, guidelines: &["MG-6.3"], experimental: false, native: false, about: "Discourages legacy 'reg' and 'wire' keywords in SystemVerilog testbenches in favor of 'logic'.", check: Check::Syntax(legacy_reg_wire_in_testbench::legacy_reg_wire),
        },
        Rule { id: "SV-DEP-001", name: "ProgramBlockDeprecation", category: "Deprecated", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &["MG-6.3"], experimental: false, native: false, about: "Disallow the obsolete SystemVerilog 'program' block. Modern verification architectures mandate 'module' with clocking blocks to prevent premature simulation termination.", check: Check::Syntax(program_block_deprecation::program_block),
        },
        Rule { id: "SV-DEP-002", name: "DefparamDeprecation", category: "Deprecated", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &["MG-6.3"], experimental: false, native: false, about: "Disallow 'defparam' statements. Parameter overrides must be specified directly on instance headers.", check: Check::Syntax(defparam_deprecation::defparam) },
        Rule { id: "SV-DEP-003", name: "LegacyAlwaysBlock", category: "Deprecated", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-6.3"], experimental: false, native: false, about: "Disallow untyped 'always' blocks. Enforces IEEE 1800 intent-specific blocks: always_comb, always_ff, or always_latch.", check: Check::Syntax(legacy_always_block::legacy_always) },
        Rule { id: "SV-DEP-004", name: "RandomSystemFunction", category: "Deprecated", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-6.3"], experimental: false, native: false, about: "Disallow legacy Verilog '$random'. Replaced by '$urandom' and '$urandom_range' in IEEE 1800.", check: Check::Syntax(random_system_function::random_system_function),
        },
        Rule { id: "SV-DEP-005", name: "ProceduralContinuousAssign", category: "Deprecated", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &["MG-6.3"], experimental: false, native: false, about: "Disallow procedural 'assign' and 'deassign' statements. Procedural continuous assignments are non-synthesizable and induce simulation race conditions.", check: Check::Syntax(procedural_continuous_assign::procedural_assign),
        },
        Rule { id: "SV-DEP-006", name: "NonAnsiPortList", category: "Deprecated", severity: Severity::Info, profiles: METHODOLOGY, guidelines: &["MG-6.3"], experimental: false, native: false, about: "Disallow obsolete non-ANSI port list declarations. Enforces modern ANSI-style inline port lists.", check: Check::Syntax(non_ansi_port_list::non_ansi_ports) },
        Rule { id: "UVM-BAN-001", name: "UvmBannedFieldMacros", category: "Deprecated", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-6.1"], experimental: false, native: false, about: "Bans the `uvm_field_* macro suite. Mandates manual do_copy, do_compare, and do_print methods to eliminate severe simulation performance overhead and uncontrollable deep copy bugs.", check: Check::Syntax(uvm_banned_field_macros::banned_field_macros),
        },
        Rule { id: "UVM-BAN-002", name: "UvmBannedUvmDoMacros", category: "Deprecated", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-6.1"], experimental: false, native: false, about: "Discourages black-box `uvm_do macros in sequence bodies. Mandates explicit start_item / finish_item transaction handshakes.", check: Check::Syntax(uvm_banned_uvm_do_macros::banned_do_macros),
        },
        Rule { id: "UVM-DEP-001", name: "UvmDeprecatedConfigCalls", category: "Deprecated", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &["MG-6.3"], experimental: false, native: false, about: "Disallow deprecated legacy set_config_* and get_config_* methods (IEEE 1800.2 Annex H). Use type-safe uvm_config_db#(T)::set/get.", check: Check::Syntax(uvm_deprecated_config_calls::deprecated_config_calls),
        },
        Rule { id: "UVM-DEP-002", name: "UvmDeprecatedReportServer", category: "Deprecated", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-6.3"], experimental: false, native: false, about: "Disallow direct singleton calls to uvm_report_server::get_server() (IEEE 1800.2 Annex H). Obtain report servers through uvm_coreservice_t.", check: Check::Syntax(uvm_deprecated_report_server::deprecated_report_server),
        },
        Rule { id: "UVM-DEP-003", name: "UvmDeprecatedPrinterKnobs", category: "Deprecated", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-6.3"], experimental: false, native: false, about: "Disallow direct access to 'printer.knobs' (IEEE 1800.2 § 5.5.2). Use accessor methods on uvm_printer.", check: Check::Syntax(uvm_deprecated_printer_knobs::deprecated_printer_knobs),
        },
        Rule { id: "UVM-DEP-004", name: "UvmDeprecatedTestDone", category: "Deprecated", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &["MG-6.3"], experimental: false, native: false, about: "Disallow obsolete OVM 'uvm_test_done' objection handle (IEEE 1800.2 Annex H). Attach objections to the active phase parameter.", check: Check::Syntax(uvm_deprecated_test_done::deprecated_test_done),
        },
    ]
}
