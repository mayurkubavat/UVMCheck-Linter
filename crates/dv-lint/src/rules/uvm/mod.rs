//! The uvm rules: the Rust twins of `cpp/src/rules/uvm/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

mod accellera_only_api;
mod analysis_fifo_drain_starvation;
mod analysis_transaction_mutation;
mod blocking_in_observer;
mod build_phase_cross_component_access;
mod component_name_mismatch;
mod component_null_parent_in_create;
mod config_db_cross_component_resolver;
mod config_db_get_unchecked;
mod deprecated_uvm_api;
mod deprecated_uvm_macro;
mod direct_do_method_call;
mod direct_ral_mirror_tampering;
mod empty_sequence_body;
mod global_factory_override_pollution;
mod info_at_none_verbosity;
mod low_verbosity_debug_report;
mod objection_control_flow_mismatch;
mod objection_in_non_run_phase;
mod objection_without_description;
mod observer_state_mutation_in_write;
mod orphan_subsequence_objection_hang;
mod parameterized_class_factory_registration;
mod phase_objection_mismatch;
mod prohibited_display_in_uvm;
mod ral_map_address_interval_overlap;
mod ral_predictor_adapter_wiring;
mod scoreboard_leftover_queue;
mod scoreboard_silent_drop;
mod sequence_abort_safety;
mod sequence_body_without_start_item;
mod sequence_in_function_phase;
mod sequence_item_create_without_type;
mod sequence_item_default_new;
mod sequence_item_type_compatibility;
mod sequence_priority_range;
mod sequence_start_on_null_sequencer;
mod super_build_phase_order;
mod time_in_non_time_phase;
mod tlm_topology_checker;
mod unchecked_get_response;
mod unchecked_p_sequencer_cast;
mod unused_sequence;
mod unused_sequence_item;
mod uvm_component_direct_new;
mod uvm_config_db_global_wildcard;
mod uvm_config_db_null_context;
mod uvm_config_db_type_mismatch;
mod uvm_dangling_config_string;
mod uvm_factory_macro_mismatch;
mod uvm_factory_missing_utils;
mod uvm_missing_super_phase;
mod uvm_objection_in_loop;
mod uvm_p_sequencer_abuse;
mod uvm_phase_jump_hazard;
mod uvm_phase_signature_mismatch;
mod uvm_sequence_direct_vif_access;
mod uvm_sequence_pre_post_body_misuse;
mod uvm_time_in_function_phase;
mod uvm_tlm_try_get_unchecked;
mod uvm_transaction_aliasing;
mod uvm_unconnected_tlm_port;
mod uvm_unpaired_objection;
mod virtual_interface_modport_drive_inversion;
mod virtual_sequencer_wiring_checker;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "RAL-MAP-002", name: "RalMapAddressIntervalOverlap", check: Check::Mixed(ral_map_address_interval_overlap::ral_map_overlap) },
        Rule { id: "RAL-WIRE-001", name: "RalPredictorAdapterWiring", check: Check::Model(ral_predictor_adapter_wiring::ral_predictor_adapter_wiring) },
        Rule { id: "UVM-ALS-001", name: "UvmTransactionAliasing", check: Check::Syntax(uvm_transaction_aliasing::transaction_aliasing) },
        Rule { id: "UVM-API-001", name: "AccelleraOnlyApi", check: Check::Mixed(accellera_only_api::accellera_only_api) },
        Rule { id: "UVM-API-002", name: "DeprecatedUvmMacro", check: Check::Mixed(deprecated_uvm_macro::deprecated_uvm_macro) },
        Rule { id: "UVM-API-003", name: "DeprecatedUvmApi", check: Check::Mixed(deprecated_uvm_api::deprecated_uvm_api) },
        Rule { id: "UVM-BAN-003", name: "DirectDoMethodCall", check: Check::Model(direct_do_method_call::direct_do_method_call) },
        Rule { id: "UVM-CFG-001", name: "UvmConfigDbTypeMismatch", check: Check::Mixed(uvm_config_db_type_mismatch::config_db_type_mismatch) },
        Rule { id: "UVM-CFG-002", name: "UvmConfigDbNullContext", check: Check::Model(uvm_config_db_null_context::uvm_config_db_null_context) },
        Rule { id: "UVM-CFG-003", name: "UvmConfigDbGlobalWildcard", check: Check::Syntax(uvm_config_db_global_wildcard::config_db_wildcard) },
        Rule { id: "UVM-CFG-004", name: "ConfigDbGetUnchecked", check: Check::Syntax(config_db_get_unchecked::config_db_get_unchecked) },
        Rule { id: "UVM-CFG-005", name: "ConfigDbCrossComponentResolver", check: Check::Mixed(config_db_cross_component_resolver::config_db_cross_component) },
        Rule { id: "UVM-DAG-001", name: "UvmDanglingConfigString", check: Check::Mixed(uvm_dangling_config_string::dangling_config_string) },
        Rule { id: "UVM-FAC-001", name: "UvmComponentDirectNew", check: Check::Syntax(uvm_component_direct_new::component_direct_new) },
        Rule { id: "UVM-FAC-002", name: "UvmFactoryMacroMismatch", check: Check::Model(uvm_factory_macro_mismatch::uvm_factory_macro_mismatch) },
        Rule { id: "UVM-FAC-003", name: "UvmFactoryMissingUtils", check: Check::Model(uvm_factory_missing_utils::uvm_factory_missing_utils) },
        Rule { id: "UVM-FAC-004", name: "ParameterizedClassFactoryRegistration", check: Check::Mixed(parameterized_class_factory_registration::param_class_factory) },
        Rule { id: "UVM-FAC-005", name: "GlobalFactoryOverridePollution", check: Check::Model(global_factory_override_pollution::global_factory_override) },
        Rule { id: "UVM-LOG-002", name: "ProhibitedDisplayInUvm", check: Check::Mixed(prohibited_display_in_uvm::display_in_uvm) },
        Rule { id: "UVM-OBJ-001", name: "UvmUnpairedObjection", check: Check::Model(uvm_unpaired_objection::uvm_unpaired_objection) },
        Rule { id: "UVM-OBJ-002", name: "UvmObjectionInLoop", check: Check::Mixed(uvm_objection_in_loop::objection_in_loop) },
        Rule { id: "UVM-OBJ-003", name: "ObjectionControlFlowMismatch", check: Check::Mixed(objection_control_flow_mismatch::objection_control_flow) },
        Rule { id: "UVM-OBJ-004", name: "OrphanSubsequenceObjectionHang", check: Check::Mixed(orphan_subsequence_objection_hang::orphan_objection) },
        Rule { id: "UVM-OBJ-005", name: "ObjectionWithoutDescription", check: Check::Mixed(objection_without_description::objection_without_description) },
        Rule { id: "UVM-OBS-001", name: "ObserverStateMutationInWrite", check: Check::Model(observer_state_mutation_in_write::observer_state_mutation_in_write) },
        Rule { id: "UVM-OBS-002", name: "BlockingInObserver", check: Check::Mixed(blocking_in_observer::blocking_in_observer) },
        Rule { id: "UVM-PHS-001", name: "UvmMissingSuperPhase", check: Check::Model(uvm_missing_super_phase::uvm_missing_super_phase) },
        Rule { id: "UVM-PHS-002", name: "UvmTimeInFunctionPhase", check: Check::Model(uvm_time_in_function_phase::uvm_time_in_function_phase) },
        Rule { id: "UVM-PHS-003", name: "UvmPhaseSignatureMismatch", check: Check::Model(uvm_phase_signature_mismatch::uvm_phase_signature_mismatch) },
        Rule { id: "UVM-PHS-004", name: "SequenceInFunctionPhase", check: Check::Syntax(sequence_in_function_phase::sequence_in_function_phase) },
        Rule { id: "UVM-PHS-005", name: "SuperBuildPhaseOrder", check: Check::Mixed(super_build_phase_order::super_build_phase_order) },
        Rule { id: "UVM-PHS-006", name: "PhaseObjectionMismatch", check: Check::Syntax(phase_objection_mismatch::phase_objection_mismatch) },
        Rule { id: "UVM-PHS-007", name: "UvmPhaseJumpHazard", check: Check::Syntax(uvm_phase_jump_hazard::phase_jump) },
        Rule { id: "UVM-PHS-008", name: "TimeInNonTimePhase", check: Check::Syntax(time_in_non_time_phase::time_in_non_time_phase) },
        Rule { id: "UVM-PHS-009", name: "ObjectionInNonRunPhase", check: Check::Model(objection_in_non_run_phase::objection_in_non_run_phase) },
        Rule { id: "UVM-PHS-010", name: "BuildPhaseCrossComponentAccess", check: Check::Mixed(build_phase_cross_component_access::build_phase_cross_access) },
        Rule { id: "UVM-RAL-001", name: "DirectRalMirrorTampering", check: Check::Mixed(direct_ral_mirror_tampering::ral_mirror_tampering) },
        Rule { id: "UVM-REP-005", name: "LowVerbosityDebugReport", check: Check::Mixed(low_verbosity_debug_report::low_verbosity_debug) },
        Rule { id: "UVM-REP-009", name: "InfoAtNoneVerbosity", check: Check::Syntax(info_at_none_verbosity::info_at_none) },
        Rule { id: "UVM-SCB-001", name: "ScoreboardSilentDrop", check: Check::Mixed(scoreboard_silent_drop::scoreboard_silent_drop) },
        Rule { id: "UVM-SCB-002", name: "ScoreboardLeftoverQueue", check: Check::Mixed(scoreboard_leftover_queue::scoreboard_leftover_queue) },
        Rule { id: "UVM-SEQ-001", name: "UvmSequenceDirectVifAccess", check: Check::Model(uvm_sequence_direct_vif_access::uvm_sequence_direct_vif_access) },
        Rule { id: "UVM-SEQ-002", name: "UvmSequencePrePostBodyMisuse", check: Check::Model(uvm_sequence_pre_post_body_misuse::uvm_sequence_pre_post_body_misuse) },
        Rule { id: "UVM-SEQ-003", name: "UvmPSequencerAbuse", check: Check::Model(uvm_p_sequencer_abuse::uvm_p_sequencer_abuse) },
        Rule { id: "UVM-SEQ-004", name: "SequenceAbortSafety", check: Check::Syntax(sequence_abort_safety::sequence_abort_safety) },
        Rule { id: "UVM-SEQ-005", name: "EmptySequenceBody", check: Check::Mixed(empty_sequence_body::empty_sequence_body) },
        Rule { id: "UVM-SEQ-006", name: "UncheckedGetResponse", check: Check::Syntax(unchecked_get_response::unchecked_get_response) },
        Rule { id: "UVM-SEQ-007", name: "SequencePriorityRange", check: Check::Syntax(sequence_priority_range::sequence_priority) },
        Rule { id: "UVM-SEQ-008", name: "SequenceItemDefaultNew", check: Check::Syntax(sequence_item_default_new::sequence_item_default_new) },
        Rule { id: "UVM-SEQ-009", name: "SequenceBodyWithoutStartItem", check: Check::Mixed(sequence_body_without_start_item::body_without_start_item) },
        Rule { id: "UVM-SEQ-010", name: "SequenceItemCreateWithoutType", check: Check::Mixed(sequence_item_create_without_type::item_create_without_type) },
        Rule { id: "UVM-SEQ-011", name: "UncheckedPSequencerCast", check: Check::Mixed(unchecked_p_sequencer_cast::unchecked_p_sequencer_cast) },
        Rule { id: "UVM-SEQ-012", name: "SequenceStartOnNullSequencer", check: Check::Syntax(sequence_start_on_null_sequencer::start_on_null) },
        Rule { id: "UVM-SEQ-013", name: "SequenceItemTypeCompatibility", check: Check::Syntax(sequence_item_type_compatibility::item_type_compatibility) },
        Rule { id: "UVM-SEQ-014", name: "UnusedSequence", check: Check::Mixed(unused_sequence::unused_sequence) },
        Rule { id: "UVM-SEQ-015", name: "UnusedSequenceItem", check: Check::Mixed(unused_sequence_item::unused_sequence_item) },
        Rule { id: "UVM-STR-005", name: "ComponentNullParentInCreate", check: Check::Syntax(component_null_parent_in_create::null_parent_create) },
        Rule { id: "UVM-STY-003", name: "ComponentNameMismatch", check: Check::Mixed(component_name_mismatch::component_name_mismatch) },
        Rule { id: "UVM-TLM-001", name: "UvmUnconnectedTlmPort", check: Check::Model(uvm_unconnected_tlm_port::uvm_unconnected_tlm_port) },
        Rule { id: "UVM-TLM-002", name: "UvmTlmTryGetUnchecked", check: Check::Syntax(uvm_tlm_try_get_unchecked::tlm_try_unchecked) },
        Rule { id: "UVM-TLM-004", name: "TlmTopologyChecker", check: Check::Mixed(tlm_topology_checker::tlm_topology) },
        Rule { id: "UVM-TLM-005", name: "AnalysisTransactionMutation", check: Check::Mixed(analysis_transaction_mutation::analysis_mutation) },
        Rule { id: "UVM-TLM-006", name: "AnalysisFifoDrainStarvation", check: Check::Mixed(analysis_fifo_drain_starvation::fifo_drain_starvation) },
        Rule { id: "UVM-VIF-DIR", name: "VirtualInterfaceModportDriveInversion", check: Check::Model(virtual_interface_modport_drive_inversion::virtual_interface_modport_drive_inversion) },
        Rule { id: "UVM-VSEQ-001", name: "VirtualSequencerWiringChecker", check: Check::Model(virtual_sequencer_wiring_checker::virtual_sequencer_wiring_checker) },
    ]
}
