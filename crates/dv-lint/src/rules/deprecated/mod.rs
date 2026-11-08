//! The deprecated rules: the Rust twins of `cpp/src/rules/deprecated/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

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
        Rule { id: "SV-BAN-001", name: "LegacyRegWireInTestbench", check: Check::Syntax(legacy_reg_wire_in_testbench::legacy_reg_wire) },
        Rule { id: "SV-DEP-001", name: "ProgramBlockDeprecation", check: Check::Syntax(program_block_deprecation::program_block) },
        Rule { id: "SV-DEP-002", name: "DefparamDeprecation", check: Check::Syntax(defparam_deprecation::defparam) },
        Rule { id: "SV-DEP-003", name: "LegacyAlwaysBlock", check: Check::Syntax(legacy_always_block::legacy_always) },
        Rule { id: "SV-DEP-004", name: "RandomSystemFunction", check: Check::Syntax(random_system_function::random_system_function) },
        Rule { id: "SV-DEP-005", name: "ProceduralContinuousAssign", check: Check::Syntax(procedural_continuous_assign::procedural_assign) },
        Rule { id: "SV-DEP-006", name: "NonAnsiPortList", check: Check::Syntax(non_ansi_port_list::non_ansi_ports) },
        Rule { id: "UVM-BAN-001", name: "UvmBannedFieldMacros", check: Check::Syntax(uvm_banned_field_macros::banned_field_macros) },
        Rule { id: "UVM-BAN-002", name: "UvmBannedUvmDoMacros", check: Check::Syntax(uvm_banned_uvm_do_macros::banned_do_macros) },
        Rule { id: "UVM-DEP-001", name: "UvmDeprecatedConfigCalls", check: Check::Syntax(uvm_deprecated_config_calls::deprecated_config_calls) },
        Rule { id: "UVM-DEP-002", name: "UvmDeprecatedReportServer", check: Check::Syntax(uvm_deprecated_report_server::deprecated_report_server) },
        Rule { id: "UVM-DEP-003", name: "UvmDeprecatedPrinterKnobs", check: Check::Syntax(uvm_deprecated_printer_knobs::deprecated_printer_knobs) },
        Rule { id: "UVM-DEP-004", name: "UvmDeprecatedTestDone", check: Check::Syntax(uvm_deprecated_test_done::deprecated_test_done) },
    ]
}
