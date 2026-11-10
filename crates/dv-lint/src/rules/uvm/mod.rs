//! The uvm rules: the Rust twins of `cpp/src/rules/uvm/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::profile::*;
use crate::{Check, Rule, Severity};

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
pub(crate) mod tlm_topology_checker;
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
        Rule { id: "RAL-MAP-002", name: "RalMapAddressIntervalOverlap", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Detects 1D address interval overlaps and register aliasing collisions within the same uvm_reg_map in a uvm_reg_block, preventing silent register corruption in hardware.", check: Check::Mixed(ral_map_address_interval_overlap::ral_map_overlap),
        },
        Rule { id: "RAL-WIRE-001", name: "RalPredictorAdapterWiring", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-5.1"], experimental: false, native: false, about: "Verifies that all instantiated uvm_reg_predictor components have their .map property assigned in connect_phase. Without a map binding, explicit register prediction fails silently and register mirror values will never be updated during simulation.", check: Check::Model(ral_predictor_adapter_wiring::ral_predictor_adapter_wiring),
        },
        Rule { id: "UVM-ALS-001", name: "UvmTransactionAliasing", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-3.1"], experimental: false, native: false, about: "Disallows in-place mutation of transaction objects after broadcasting via an analysis port (.write()). Analysis ports pass shared references; subsequent in-place mutations corrupt downstream scoreboards and subscribers.", check: Check::Syntax(uvm_transaction_aliasing::transaction_aliasing),
        },
        Rule { id: "UVM-API-001", name: "AccelleraOnlyApi", category: "Portability", severity: Severity::Info, profiles: SOUNDNESS | SECURITY, guidelines: &[], experimental: false, native: false, about: "Flags calls to, and classes extending, parts of the Accellera UVM library that IEEE 1800.2 does not define (tagged @uvm-accellera in the library source), such as uvm_component::print_config or uvm_port_base::debug_connected_to. Code that relies on them is tied to one UVM implementation and release. The list comes from the UVM API pack of the version being compiled.", check: Check::Mixed(accellera_only_api::accellera_only_api),
        },
        Rule { id: "UVM-API-002", name: "DeprecatedUvmMacro", category: "Deprecated", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Flags uses of UVM macros that the UVM version being compiled defines only under UVM_ENABLE_DEPRECATED_API (in UVM 2020: `uvm_do_with, `uvm_do_on, `uvm_do_pri, `uvm_create_on, `uvm_send_pri and the rest of that family). Without the define the code does not compile. The list comes from the UVM API pack of the version being compiled.", check: Check::Mixed(deprecated_uvm_macro::deprecated_uvm_macro),
        },
        Rule { id: "UVM-API-003", name: "DeprecatedUvmApi", category: "Deprecated", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: true, about: "Calls to UVM methods, and classes extending UVM classes, that the compiled UVM release keeps only as deprecated API (UVM 1.1d/1.2: what `UVM_NO_DEPRECATED` removes, such as `uvm_component::set_config_int` or `uvm_random_sequence`). Read from the release's API pack in `data/uvm-api/`.", check: Check::Mixed(deprecated_uvm_api::deprecated_uvm_api),
        },
        Rule { id: "UVM-BAN-003", name: "DirectDoMethodCall", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Flags direct calls to do_copy, do_compare, do_print, do_pack, do_unpack and do_record. These are hooks for the public copy/compare/print/pack/record methods; calling them directly bypasses the policy object. super.do_*() inside an override is allowed.", check: Check::Model(direct_do_method_call::direct_do_method_call),
        },
        Rule { id: "UVM-CFG-001", name: "UvmConfigDbTypeMismatch", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &["MG-2.4"], experimental: false, native: false, about: "The type parameter in uvm_config_db#(T)::get must match the type parameter used in the corresponding set call for the same field name.", check: Check::Mixed(uvm_config_db_type_mismatch::config_db_type_mismatch),
        },
        Rule { id: "UVM-CFG-002", name: "UvmConfigDbNullContext", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Passing 'null' as context to uvm_config_db inside a uvm_component breaks hierarchical scoping and prevents targeted configuration overrides. Use 'this' instead.", check: Check::Model(uvm_config_db_null_context::uvm_config_db_null_context),
        },
        Rule { id: "UVM-CFG-003", name: "UvmConfigDbGlobalWildcard", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-2.3"], experimental: false, native: false, about: "Calling uvm_config_db::set(null, \"*\", ...) with a null context and a bare wildcard or empty scope triggers global regex matching across every component in the testbench, severely degrading simulation startup.", check: Check::Syntax(uvm_config_db_global_wildcard::config_db_wildcard),
        },
        Rule { id: "UVM-CFG-004", name: "ConfigDbGetUnchecked", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-8.1"], experimental: false, native: false, about: "Flags calls to 'uvm_config_db#(...)::get()' where the boolean return status is ignored without being checked in an 'if' condition or assertion.", check: Check::Syntax(config_db_get_unchecked::config_db_get_unchecked),
        },
        Rule { id: "UVM-CFG-005", name: "ConfigDbCrossComponentResolver", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &["MG-2.3"], experimental: false, native: false, about: "Performs cross-component semantic resolution on uvm_config_db calls across the testbench hierarchy DAG to detect configuration type mismatches between set() and get() calls.", check: Check::Mixed(config_db_cross_component_resolver::config_db_cross_component),
        },
        Rule { id: "UVM-DAG-001", name: "UvmDanglingConfigString", category: "UVM", severity: Severity::Error, profiles: SOUNDNESS | METHODOLOGY, guidelines: &["MG-2.4"], experimental: false, native: false, about: "Static UVM component DAG path matching: detects glob strings in uvm_config_db::set() that match zero instantiated components in the verification hierarchy. Typographical errors in string paths cause configurations to be silently dropped.", check: Check::Mixed(uvm_dangling_config_string::dangling_config_string),
        },
        Rule { id: "UVM-FAC-001", name: "UvmComponentDirectNew", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &["MG-4.5"], experimental: false, native: false, about: "UVM components must be created using the UVM factory (type_name::type_id::create) instead of direct new() calls to support testbench overrides.", check: Check::Syntax(uvm_component_direct_new::component_direct_new),
        },
        Rule { id: "UVM-FAC-002", name: "UvmFactoryMacroMismatch", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &["MG-4.5"], experimental: false, native: false, about: "Classes derived from uvm_component must use `uvm_component_utils, and classes derived from uvm_object (non-component) must use `uvm_object_utils.", check: Check::Model(uvm_factory_macro_mismatch::uvm_factory_macro_mismatch),
        },
        Rule { id: "UVM-FAC-003", name: "UvmFactoryMissingUtils", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-4.5"], experimental: false, native: false, about: "Concrete classes derived from uvm_object or uvm_component must declare factory registration macros (`uvm_component_utils / `uvm_object_utils).", check: Check::Mixed(uvm_factory_missing_utils::uvm_factory_missing_utils),
        },
        Rule { id: "UVM-FAC-004", name: "ParameterizedClassFactoryRegistration", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &["MG-4.5"], experimental: false, native: false, about: "Parameterized UVM components and objects must use `uvm_component_param_utils or `uvm_object_param_utils instead of non-parameterized utils macros.", check: Check::Mixed(parameterized_class_factory_registration::param_class_factory),
        },
        Rule { id: "UVM-FAC-005", name: "GlobalFactoryOverridePollution", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-4.5"], experimental: false, native: false, about: "Flags global factory type overrides (set_type_override, set_type_override_by_type, factory.set_type_override_by_name, T::type_id::set_type_override) outside uvm_test classes. They change the type for every test and VIP in the simulation; reusable components and sequences should use instance overrides scoped to their sub-tree.", check: Check::Model(global_factory_override_pollution::global_factory_override),
        },
        Rule { id: "UVM-LOG-002", name: "ProhibitedDisplayInUvm", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-8.1"], experimental: false, native: false, about: "Disallow direct $display/$write/$strobe calls inside UVM classes (components, objects, sequences). SystemVerilog display tasks bypass UVM reporting infrastructure, log redirection, and verbosity filtering.", check: Check::Mixed(prohibited_display_in_uvm::display_in_uvm),
        },
        Rule { id: "UVM-OBJ-001", name: "UvmUnpairedObjection", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Every phase.raise_objection() call must have a corresponding phase.drop_objection() within the same execution context to prevent simulation hangs or fatal errors.", check: Check::Mixed(uvm_unpaired_objection::uvm_unpaired_objection),
        },
        Rule { id: "UVM-OBJ-002", name: "UvmObjectionInLoop", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Avoid raising and dropping objections inside high-frequency transaction loops to prevent severe simulation performance degradation.", check: Check::Mixed(uvm_objection_in_loop::objection_in_loop),
        },
        Rule { id: "UVM-OBJ-003", name: "ObjectionControlFlowMismatch", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Performs control-flow graph (CFG) analysis on UVM phase and task routines to verify that every execution path containing raise_objection() is strictly guaranteed to reach a matching drop_objection() without early returns or asymmetric conditional branching.", check: Check::Mixed(objection_control_flow_mismatch::objection_control_flow),
        },
        Rule { id: "UVM-OBJ-004", name: "OrphanSubsequenceObjectionHang", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Detects background processes launched via fork..join_none inside UVM sequences that raise objections without a deterministic drop path, leaving orphan objections that hang simulation phases.", check: Check::Mixed(orphan_subsequence_objection_hang::orphan_objection),
        },
        Rule { id: "UVM-OBJ-005", name: "ObjectionWithoutDescription", category: "UVM", severity: Severity::Info, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Flags raise_objection()/drop_objection() on a phase or objection with no description or an empty one. A short description makes objection traces of a hung test readable.", check: Check::Mixed(objection_without_description::objection_without_description),
        },
        Rule { id: "UVM-OBS-001", name: "ObserverStateMutationInWrite", category: "Architecture", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &["MG-3.1"], experimental: false, native: false, about: "Mutating the observed transaction object inside a subscriber's write() method corrupts state for other concurrent subscribers in the Observer pattern. Clone the transaction before mutating.", check: Check::Model(observer_state_mutation_in_write::observer_state_mutation_in_write),
        },
        Rule { id: "UVM-OBS-002", name: "BlockingInObserver", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &["MG-5.1"], experimental: false, native: false, about: "Analysis write() implementations (uvm_subscriber::write and the write<SFX> methods behind uvm_analysis_imp<SFX>) must be functions that finish in zero time. Flags a write declared as a task, and a write that forks a thread per transaction.", check: Check::Mixed(blocking_in_observer::blocking_in_observer),
        },
        Rule { id: "UVM-PHS-001", name: "UvmMissingSuperPhase", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Overridden standard UVM phases (e.g., build_phase, connect_phase) in uvm_component must call super.<phase_name>(phase) to ensure proper base-class configuration.", check: Check::Mixed(uvm_missing_super_phase::uvm_missing_super_phase),
        },
        Rule { id: "UVM-PHS-002", name: "UvmTimeInFunctionPhase", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Function-based UVM phases (e.g., build_phase, connect_phase) must not consume simulation time using delays (#), event controls (@), or wait statements.", check: Check::Model(uvm_time_in_function_phase::uvm_time_in_function_phase),
        },
        Rule { id: "UVM-PHS-003", name: "UvmPhaseSignatureMismatch", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Standard UVM phase methods must declare correct signatures (function void vs task, and a single argument of type uvm_phase).", check: Check::Model(uvm_phase_signature_mismatch::uvm_phase_signature_mismatch),
        },
        Rule { id: "UVM-PHS-004", name: "SequenceInFunctionPhase", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Disallow starting sequences (.start()) inside zero-time function phases (build_phase, connect_phase, etc.). Sequences require time-consuming simulation events and must be executed in task phases.", check: Check::Syntax(sequence_in_function_phase::sequence_in_function_phase),
        },
        Rule { id: "UVM-PHS-005", name: "SuperBuildPhaseOrder", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Enforces that super.build_phase(phase) is the very first procedural statement in build_phase. Top-down phasing requires base class build_phase to execute before derived class configuration logic.", check: Check::Mixed(super_build_phase_order::super_build_phase_order),
        },
        Rule { id: "UVM-PHS-006", name: "PhaseObjectionMismatch", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Detects 'phase.raise_objection()' calls without a matching 'phase.drop_objection()' in the same task or method. Unpaired objections prevent UVM phases from terminating, causing simulation hangs.", check: Check::Mixed(phase_objection_mismatch::phase_objection_mismatch),
        },
        Rule { id: "UVM-PHS-007", name: "UvmPhaseJumpHazard", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Disallows manual phase jumping ('phase.jump(...)') in UVM testbenches due to objection leaks and scoreboard desynchronization risks.", check: Check::Syntax(uvm_phase_jump_hazard::phase_jump) },
        Rule { id: "UVM-PHS-008", name: "TimeInNonTimePhase", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Disallows time-consuming statements (#, @, wait, ##) inside non-time-consuming UVM phases (e.g. build_phase, connect_phase, check_phase).", check: Check::Syntax(time_in_non_time_phase::time_in_non_time_phase),
        },
        Rule { id: "UVM-PHS-009", name: "ObjectionInNonRunPhase", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Disallows calling raise_objection() or drop_objection() inside non-run function phases (e.g. build_phase, connect_phase). Objections are strictly reserved for runtime phases.", check: Check::Model(objection_in_non_run_phase::objection_in_non_run_phase),
        },
        Rule { id: "UVM-PHS-010", name: "BuildPhaseCrossComponentAccess", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-2.1"], experimental: false, native: false, about: "Detects direct access to sibling components or cross-component handles during build_phase. Because the execution order of build_phase among sibling components is undefined in UVM, referencing sibling state during build_phase results in non-deterministic initialization races.", check: Check::Mixed(build_phase_cross_component_access::build_phase_cross_access),
        },
        Rule { id: "UVM-RAL-001", name: "DirectRalMirrorTampering", category: "Architecture", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Direct assignment to internal RAL fields (m_value, m_mirrored, m_desired) bypasses register callbacks, access policy checks, and prediction pipelines. Use predict() or bus operations instead.", check: Check::Mixed(direct_ral_mirror_tampering::ral_mirror_tampering),
        },
        Rule { id: "UVM-REP-005", name: "LowVerbosityDebugReport", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Flags UVM debug or trace messages configured with low verbosity (UVM_LOW or UVM_NONE).", check: Check::Mixed(low_verbosity_debug_report::low_verbosity_debug),
        },
        Rule { id: "UVM-REP-009", name: "InfoAtNoneVerbosity", category: "UVM", severity: Severity::Info, profiles: METHODOLOGY, guidelines: &[], experimental: true, native: false, about: "Flags `uvm_info messages at UVM_NONE verbosity, which cannot be filtered with +UVM_VERBOSITY. Experimental: measuring how often real benches use UVM_NONE on purpose.", check: Check::Syntax(info_at_none_verbosity::info_at_none),
        },
        Rule { id: "UVM-SCB-001", name: "ScoreboardSilentDrop", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &["MG-8.1"], experimental: false, native: false, about: "Detects silent returns in scoreboard write methods when expected queues are empty without logging an error. Spurious or unpredicted DUT packets are dropped, hiding hardware bugs.", check: Check::Mixed(scoreboard_silent_drop::scoreboard_silent_drop),
        },
        Rule { id: "UVM-SCB-002", name: "ScoreboardLeftoverQueue", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &["MG-8.1"], experimental: false, native: false, about: "Scoreboard expected transaction queues must be checked for drain completeness in check_phase or report_phase. Missing drain checks allow dropped DUT packets to pass silently.", check: Check::Mixed(scoreboard_leftover_queue::scoreboard_leftover_queue),
        },
        Rule { id: "UVM-SEQ-001", name: "UvmSequenceDirectVifAccess", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-3.1"], experimental: false, native: false, about: "Sequences and sequence items must not directly declare or drive virtual interface signals. Signal-level driving is the sole responsibility of the driver.", check: Check::Model(uvm_sequence_direct_vif_access::uvm_sequence_direct_vif_access),
        },
        Rule { id: "UVM-SEQ-002", name: "UvmSequencePrePostBodyMisuse", category: "Architecture", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "pre_body() and post_body() callbacks in uvm_sequence are explicitly bypassed when the sequence is executed as a child sequence via `uvm_do or `uvm_do_with. Use pre_start() / post_start() or body() instead.", check: Check::Model(uvm_sequence_pre_post_body_misuse::uvm_sequence_pre_post_body_misuse),
        },
        Rule { id: "UVM-SEQ-003", name: "UvmPSequencerAbuse", category: "Architecture", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-2.1"], experimental: false, native: false, about: "Using `uvm_declare_p_sequencer or accessing p_sequencer violates the Law of Demeter by tightly coupling the sequence data tier to physical sequencer component internals. Pass configuration down through sequence fields or uvm_config_db.", check: Check::Model(uvm_p_sequencer_abuse::uvm_p_sequencer_abuse),
        },
        Rule { id: "UVM-SEQ-004", name: "SequenceAbortSafety", category: "UVM", severity: Severity::Error, profiles: SOUNDNESS | METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Detects calls to start_item() in UVM sequences that can bypass finish_item() via early return or break statements. An aborted sequence without finish_item() permanently deadlocks the sequencer's grant-wait queue.", check: Check::Syntax(sequence_abort_safety::sequence_abort_safety),
        },
        Rule { id: "UVM-SEQ-005", name: "EmptySequenceBody", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Detects sequence classes whose body() task contains no executable statements. Empty sequence bodies generate zero stimulus transactions.", check: Check::Mixed(empty_sequence_body::empty_sequence_body),
        },
        Rule { id: "UVM-SEQ-006", name: "UncheckedGetResponse", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-8.1"], experimental: false, native: false, about: "Flags calls to get_response() in UVM sequences. Calling get_response() requires the driver to actively provide response items; otherwise, the sequencer response FIFO remains empty and deadlocks simulation.", check: Check::Syntax(unchecked_get_response::unchecked_get_response),
        },
        Rule { id: "UVM-SEQ-007", name: "SequencePriorityRange", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Flags sequence start() calls passing invalid negative priority values (< -1) which corrupt sequencer arbitration.", check: Check::Syntax(sequence_priority_range::sequence_priority),
        },
        Rule { id: "UVM-SEQ-008", name: "SequenceItemDefaultNew", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-4.4"], experimental: false, native: false, about: "Requires sequence item constructor new() to provide a default value for the string name argument to support UVM factory creation and cloning.", check: Check::Syntax(sequence_item_default_new::sequence_item_default_new),
        },
        Rule { id: "UVM-SEQ-009", name: "SequenceBodyWithoutStartItem", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Flags calls to finish_item() inside uvm_sequence::body() that are not preceded by a corresponding start_item(). The UVM sequencer arbitration handshake strictly requires start_item() before finish_item().", check: Check::Mixed(sequence_body_without_start_item::body_without_start_item),
        },
        Rule { id: "UVM-SEQ-010", name: "SequenceItemCreateWithoutType", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-4.5"], experimental: false, native: false, about: "Flags sequence items created via direct new() inside sequence body(). Sequence items must be created via factory type_id::create() to allow testcase overrides.", check: Check::Mixed(sequence_item_create_without_type::item_create_without_type),
        },
        Rule { id: "UVM-SEQ-011", name: "UncheckedPSequencerCast", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-2.1"], experimental: false, native: false, about: "Flags unchecked $cast(p_sequencer, m_sequencer) calls in sequences that do not verify the return status.", check: Check::Mixed(unchecked_p_sequencer_cast::unchecked_p_sequencer_cast),
        },
        Rule { id: "UVM-SEQ-012", name: "SequenceStartOnNullSequencer", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Flags sequence start(null) calls that start sequences without an attached sequencer handle.", check: Check::Syntax(sequence_start_on_null_sequencer::start_on_null),
        },
        Rule { id: "UVM-SEQ-013", name: "SequenceItemTypeCompatibility", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Statically verifies that transaction item types between uvm_driver#(REQ) and uvm_sequencer#(REQ) are type-compatible across connect_phase connections.", check: Check::Syntax(sequence_item_type_compatibility::item_type_compatibility),
        },
        Rule { id: "UVM-SEQ-014", name: "UnusedSequence", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: true, native: false, about: "Flags a sequence that no other class in the compilation names, when the run also contains a test. Experimental: the sequence may be started from a file that was not linted.", check: Check::Mixed(unused_sequence::unused_sequence) },
        Rule { id: "UVM-SEQ-015", name: "UnusedSequenceItem", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: true, native: false, about: "Flags a sequence item that no other class in the compilation names, when the run also contains a test. Experimental: the item may be created from a file that was not linted.", check: Check::Mixed(unused_sequence_item::unused_sequence_item),
        },
        Rule { id: "UVM-STR-005", name: "ComponentNullParentInCreate", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-4.5"], experimental: false, native: false, about: "Flags UVM component factory create calls passing 'null' as the parent handle, creating orphaned hierarchy nodes.", check: Check::Syntax(component_null_parent_in_create::null_parent_create),
        },
        Rule { id: "UVM-STY-003", name: "ComponentNameMismatch", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Disallows passing hardcoded string literals to super.new() in uvm_component constructors. The constructor must forward the 'name' argument so the UVM factory can assign distinct instance names.", check: Check::Mixed(component_name_mismatch::component_name_mismatch),
        },
        Rule { id: "UVM-TLM-001", name: "UvmUnconnectedTlmPort", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-5.1"], experimental: false, native: false, about: "Mandatory TLM ports and exports declared in uvm_component must be connected in connect_phase to prevent runtime fatal errors or null-pointer dereferences.", check: Check::Model(uvm_unconnected_tlm_port::uvm_unconnected_tlm_port),
        },
        Rule { id: "UVM-TLM-002", name: "UvmTlmTryGetUnchecked", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-8.1"], experimental: false, native: false, about: "Flags calls to non-blocking TLM methods (try_get(), try_peek(), try_put()) where the boolean return status is ignored.", check: Check::Syntax(uvm_tlm_try_get_unchecked::tlm_try_unchecked),
        },
        Rule { id: "UVM-TLM-004", name: "TlmTopologyChecker", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-5.1"], experimental: false, native: false, about: "Performs cross-component elaboration topology checks to verify that mandatory TLM ports (such as driver seq_item_pull_port and monitor analysis_port) are bound in connect_phase across the verification environment hierarchy.", check: Check::Mixed(tlm_topology_checker::tlm_topology) },
        Rule { id: "UVM-TLM-005", name: "AnalysisTransactionMutation", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-3.1"], experimental: false, native: false, about: "Detects in-place modification of transactions received via analysis write() methods. Because analysis ports broadcast by reference, modifying transaction fields directly causes silent data corruption across other concurrent subscribers and scoreboards.", check: Check::Mixed(analysis_transaction_mutation::analysis_mutation),
        },
        Rule { id: "UVM-TLM-006", name: "AnalysisFifoDrainStarvation", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Detects instantiated uvm_tlm_analysis_fifo components that receive transactions via connect_phase but have zero drain operations (.get(), .try_get()) across all task-based phases, causing unbounded simulation heap memory growth and fatal bad_alloc crashes.", check: Check::Mixed(analysis_fifo_drain_starvation::fifo_drain_starvation),
        },
        Rule { id: "UVM-VIF-DIR", name: "VirtualInterfaceModportDriveInversion", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &["MG-3.1"], experimental: false, native: false, about: "Detects procedural signal driving (via '=' or '<=') on virtual interface handles inside passive uvm_monitor components. Monitors must remain purely passive observers to avoid multi-driver electrical contention and X-propagation.", check: Check::Mixed(virtual_interface_modport_drive_inversion::virtual_interface_modport_drive_inversion),
        },
        Rule { id: "UVM-VSEQ-001", name: "VirtualSequencerWiringChecker", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-5.1"], experimental: false, native: false, about: "Statically verifies that sub-sequencer handles declared within a virtual sequencer are fully wired and assigned in connect_phase across the verification environment.", check: Check::Model(virtual_sequencer_wiring_checker::virtual_sequencer_wiring_checker),
        },
    ]
}
