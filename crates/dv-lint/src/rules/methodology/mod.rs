//! The methodology rules: the Rust twins of `cpp/src/rules/methodology/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

mod absolute_config_db_path;
mod agent_ignores_is_active;
mod agent_without_config_object;
mod callback_not_registered;
mod change_coupling;
mod check_failure_as_warning;
mod clone_fragments;
mod config_db_as_runtime_channel;
mod config_db_get_without_set;
mod config_db_set_never_got;
mod config_knob_budget;
mod copied_agent_package;
mod coverage_outside_observers;
mod define_for_constant;
mod driver_generates_stimulus;
mod duplicated_constraint;
mod dut_checking_in_agent;
mod fatal_in_checker_path;
mod feature_envy;
mod file_class_name_mismatch;
mod foreign_component_state_write;
mod function_output_arguments;
mod hand_rolled_singleton;
mod handle_to_env_or_test;
mod hardcoded_topology;
mod hierarchical_reference_in_class;
mod hotspot;
mod ifdef_in_reusable_class;
mod implicit_enum_base_type;
mod incomplete_item_contract;
mod incomplete_reg_adapter;
mod interface_without_assertions;
mod legacy_integer_type;
mod macro_as_routine;
mod manual_bit_field_slicing;
mod monitor_depends_on_active_path;
mod no_extension_point;
mod override_by_name_string;
mod package_layer_inversion;
mod repeated_string_literal;
mod run_phase_with_runtime_phases;
mod scattered_config_key_literal;
mod sequencer_from_config_db;
mod shared_config_mutated_at_runtime;
mod soft_constraint_shadowing;
mod string_as_enum;
mod string_mode_parameter;
mod test_builds_leaf_components;
mod unstable_package_depended_on;
mod unused_package_import;
mod virtual_interface_in_checker;
mod wildcard_import_outside_package;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "RAL-ADP-001", name: "IncompleteRegAdapter", check: Check::Mixed(incomplete_reg_adapter::incomplete_reg_adapter) },
        Rule { id: "SV-CNS-001", name: "DefineForConstant", check: Check::Mixed(define_for_constant::define_for_constant) },
        Rule { id: "SV-CNS-002", name: "MacroAsRoutine", check: Check::Mixed(macro_as_routine::macro_as_routine) },
        Rule { id: "SV-CNS-003", name: "IfdefInReusableClass", check: Check::Mixed(ifdef_in_reusable_class::ifdef_in_reusable_class) },
        Rule { id: "SV-CNS-004", name: "StringAsEnum", check: Check::Mixed(string_as_enum::string_as_enum) },
        Rule { id: "SV-CNS-005", name: "ManualBitFieldSlicing", check: Check::Mixed(manual_bit_field_slicing::manual_bit_field_slicing) },
        Rule { id: "SV-CNS-006", name: "ImplicitEnumBaseType", check: Check::Mixed(implicit_enum_base_type::implicit_enum_base_type) },
        Rule { id: "SV-CNS-007", name: "LegacyIntegerType", check: Check::Mixed(legacy_integer_type::legacy_integer_type) },
        Rule { id: "SV-CNS-008", name: "FunctionOutputArguments", check: Check::Model(function_output_arguments::function_output_arguments) },
        Rule { id: "SV-CPL-001", name: "StringModeParameter", check: Check::Mixed(string_mode_parameter::string_mode_parameter) },
        Rule { id: "SV-DBC-001", name: "InterfaceWithoutAssertions", check: Check::Mixed(interface_without_assertions::interface_without_assertions) },
        Rule { id: "SV-DSN-011", name: "FeatureEnvy", check: Check::Mixed(feature_envy::feature_envy) },
        Rule { id: "SV-DUP-002", name: "CloneFragments", check: Check::Mixed(clone_fragments::clone_fragments) },
        Rule { id: "SV-DUP-003", name: "DuplicatedConstraint", check: Check::Mixed(duplicated_constraint::duplicated_constraint) },
        Rule { id: "SV-DUP-004", name: "RepeatedStringLiteral", check: Check::Mixed(repeated_string_literal::repeated_string_literal) },
        Rule { id: "SV-DUP-005", name: "CopiedAgentPackage", check: Check::Mixed(copied_agent_package::copied_agent_package) },
        Rule { id: "SV-EVO-001", name: "Hotspot", check: Check::Mixed(hotspot::hotspot) },
        Rule { id: "SV-EVO-002", name: "ChangeCoupling", check: Check::Mixed(change_coupling::change_coupling) },
        Rule { id: "SV-MET-009", name: "ConfigKnobBudget", check: Check::Model(config_knob_budget::config_knob_budget) },
        Rule { id: "SV-PKG-001", name: "FileClassNameMismatch", check: Check::Mixed(file_class_name_mismatch::file_class_name_mismatch) },
        Rule { id: "SV-PKG-002", name: "WildcardImportOutsidePackage", check: Check::Mixed(wildcard_import_outside_package::wildcard_import_outside_package) },
        Rule { id: "SV-PKG-003", name: "UnusedPackageImport", check: Check::Mixed(unused_package_import::unused_package_import) },
        Rule { id: "SV-PKG-004", name: "PackageLayerInversion", check: Check::Mixed(package_layer_inversion::package_layer_inversion) },
        Rule { id: "SV-PKG-005", name: "UnstablePackageDependedOn", check: Check::Mixed(unstable_package_depended_on::unstable_package_depended_on) },
        Rule { id: "SV-RND-027", name: "SoftConstraintShadowing", check: Check::Mixed(soft_constraint_shadowing::soft_constraint_shadowing) },
        Rule { id: "UVM-CFG-007", name: "ConfigDbAsRuntimeChannel", check: Check::Mixed(config_db_as_runtime_channel::config_db_as_runtime_channel) },
        Rule { id: "UVM-CFG-008", name: "ScatteredConfigKeyLiteral", check: Check::Mixed(scattered_config_key_literal::scattered_config_key_literal) },
        Rule { id: "UVM-CFG-009", name: "SharedConfigMutatedAtRuntime", check: Check::Mixed(shared_config_mutated_at_runtime::shared_config_mutated_at_runtime) },
        Rule { id: "UVM-CFG-010", name: "ConfigDbGetWithoutSet", check: Check::Mixed(config_db_get_without_set::config_db_get_without_set) },
        Rule { id: "UVM-CFG-011", name: "ConfigDbSetNeverGot", check: Check::Mixed(config_db_set_never_got::config_db_set_never_got) },
        Rule { id: "UVM-CPL-002", name: "HandleToEnvOrTest", check: Check::Model(handle_to_env_or_test::handle_to_env_or_test) },
        Rule { id: "UVM-CPL-003", name: "ForeignComponentStateWrite", check: Check::Mixed(foreign_component_state_write::foreign_component_state_write) },
        Rule { id: "UVM-CPL-004", name: "HierarchicalReferenceInClass", check: Check::Mixed(hierarchical_reference_in_class::hierarchical_reference_in_class) },
        Rule { id: "UVM-PAT-001", name: "CallbackNotRegistered", check: Check::Mixed(callback_not_registered::callback_not_registered) },
        Rule { id: "UVM-PAT-002", name: "HandRolledSingleton", check: Check::Mixed(hand_rolled_singleton::hand_rolled_singleton) },
        Rule { id: "UVM-PAT-003", name: "NoExtensionPoint", check: Check::Mixed(no_extension_point::no_extension_point) },
        Rule { id: "UVM-PHS-011", name: "RunPhaseWithRuntimePhases", check: Check::Model(run_phase_with_runtime_phases::run_phase_with_runtime_phases) },
        Rule { id: "UVM-REP-007", name: "CheckFailureAsWarning", check: Check::Mixed(check_failure_as_warning::check_failure_as_warning) },
        Rule { id: "UVM-REP-008", name: "FatalInCheckerPath", check: Check::Mixed(fatal_in_checker_path::fatal_in_checker_path) },
        Rule { id: "UVM-ROL-001", name: "DriverGeneratesStimulus", check: Check::Mixed(driver_generates_stimulus::driver_generates_stimulus) },
        Rule { id: "UVM-ROL-002", name: "CoverageOutsideObservers", check: Check::Model(coverage_outside_observers::coverage_outside_observers) },
        Rule { id: "UVM-ROL-003", name: "DutCheckingInAgent", check: Check::Model(dut_checking_in_agent::dut_checking_in_agent) },
        Rule { id: "UVM-ROL-004", name: "TestBuildsLeafComponents", check: Check::Mixed(test_builds_leaf_components::test_builds_leaf_components) },
        Rule { id: "UVM-ROL-005", name: "MonitorDependsOnActivePath", check: Check::Model(monitor_depends_on_active_path::monitor_depends_on_active_path) },
        Rule { id: "UVM-ROL-006", name: "VirtualInterfaceInChecker", check: Check::Model(virtual_interface_in_checker::virtual_interface_in_checker) },
        Rule { id: "UVM-RUS-001", name: "AgentIgnoresIsActive", check: Check::Mixed(agent_ignores_is_active::agent_ignores_is_active) },
        Rule { id: "UVM-RUS-002", name: "AgentWithoutConfigObject", check: Check::Mixed(agent_without_config_object::agent_without_config_object) },
        Rule { id: "UVM-RUS-003", name: "HardcodedTopology", check: Check::Mixed(hardcoded_topology::hardcoded_topology) },
        Rule { id: "UVM-RUS-004", name: "AbsoluteConfigDbPath", check: Check::Mixed(absolute_config_db_path::absolute_config_db_path) },
        Rule { id: "UVM-RUS-005", name: "OverrideByNameString", check: Check::Mixed(override_by_name_string::override_by_name_string) },
        Rule { id: "UVM-RUS-006", name: "IncompleteItemContract", check: Check::Mixed(incomplete_item_contract::incomplete_item_contract) },
        Rule { id: "UVM-SEQ-016", name: "SequencerFromConfigDb", check: Check::Mixed(sequencer_from_config_db::sequencer_from_config_db) },
    ]
}
