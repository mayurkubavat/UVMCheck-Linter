//! The methodology rules: the Rust twins of `cpp/src/rules/methodology/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::profile::*;
use crate::{Check, Rule, Severity};

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
        Rule { id: "RAL-ADP-001", name: "IncompleteRegAdapter", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-5.1"], experimental: false, native: false, about: "Flags uvm_reg_adapter subclasses whose bus2reg() never sets rw.status or whose reg2bus() never reads rw.kind. The register layer then reports every access as passed, or turns reads into writes (adapter pattern, MG-5.1).", check: Check::Mixed(incomplete_reg_adapter::incomplete_reg_adapter),
        },
        Rule { id: "SV-CNS-001", name: "DefineForConstant", category: "Maintainability", severity: Severity::Warning, profiles: SMELLS, guidelines: &["MG-6.1"], experimental: false, native: false, about: "Flags `define macros whose body is only a number or a string, used as constants. A package parameter or localparam has a type and a scope; a macro is global to the rest of the compilation and can be redefined silently (MG-6.1).", check: Check::Mixed(define_for_constant::define_for_constant),
        },
        Rule { id: "SV-CNS-002", name: "MacroAsRoutine", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &["MG-6.1"], experimental: false, native: false, about: "Flags multi-statement code macros that use nothing only a macro can do: no token pasting, stringification, nested macros, `__FILE__, type arguments, declarations or return/break. A function or task would be type-checked and debuggable (MG-6.1). Advisory: off by default.", check: Check::Mixed(macro_as_routine::macro_as_routine) },
        Rule { id: "SV-CNS-003", name: "IfdefInReusableClass", category: "Maintainability", severity: Severity::Warning, profiles: SMELLS, guidelines: &["MG-6.1"], experimental: false, native: false, about: "Flags `ifdef, `ifndef and `elsif inside class bodies, apart from simulator and UVM version shims. Compile-time switches multiply the variants of a class and cannot be chosen per instance; configuration can (MG-6.1).", check: Check::Mixed(ifdef_in_reusable_class::ifdef_in_reusable_class),
        },
        Rule { id: "SV-CNS-004", name: "StringAsEnum", category: "Maintainability", severity: Severity::Warning, profiles: SMELLS, guidelines: &["MG-6.2"], experimental: false, native: false, about: "Flags a string variable compared against three or more literals in one case statement or if/else-if chain. The values form an enumeration the compiler cannot check; a typo in any of them fails silently (Fowler 'primitive obsession', MG-6.2). String arguments are SV-CPL-001's.", check: Check::Mixed(string_as_enum::string_as_enum) },
        Rule { id: "SV-CNS-005", name: "ManualBitFieldSlicing", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &["MG-6.2"], experimental: false, native: false, about: "Flags a vector that is cut into three or more different fixed part-selects (hdr[7:0], hdr[15:8], hdr[31:16]). The field layout lives in scattered numbers; a packed struct names it once (MG-6.2). Advisory: off by default.", check: Check::Mixed(manual_bit_field_slicing::manual_bit_field_slicing),
        },
        Rule { id: "SV-CNS-006", name: "ImplicitEnumBaseType", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &["MG-6.3"], experimental: false, native: false, about: "Flags enums in packages, classes and interfaces declared without a base type. They silently become 32-bit int, which wastes bits in packed items and no longer matches the width of the bus field they model (MG-6.3). Advisory: off by default.", check: Check::Mixed(implicit_enum_base_type::implicit_enum_base_type),
        },
        Rule { id: "SV-CNS-007", name: "LegacyIntegerType", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &["MG-6.3"], experimental: false, native: false, about: "Flags the Verilog-1995 type 'integer' in class code. It is a 4-state 32-bit signed value; int (2-state) or an explicitly sized logic vector says what is meant (MG-6.3). Advisory: off by default.", check: Check::Mixed(legacy_integer_type::legacy_integer_type),
        },
        Rule { id: "SV-CNS-008", name: "FunctionOutputArguments", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &["MG-6.3"], experimental: false, native: false, about: "Flags void class functions that hand their results back through output arguments. A return value, or a struct for several values, keeps the call an expression and makes the data flow visible (command-query separation, MG-6.3). Overrides are left alone, since their signature is fixed. Advisory: off by default.", check: Check::Model(function_output_arguments::function_output_arguments),
        },
        Rule { id: "SV-CPL-001", name: "StringModeParameter", category: "Maintainability", severity: Severity::Warning, profiles: SMELLS, guidelines: &["MG-2.4"], experimental: false, native: false, about: "Flags routines whose string argument is compared against two or more literals to choose what to do, as in do_op(\"read\"). Caller and callee must agree on spellings the compiler never checks (control coupling, connascence of meaning, MG-2.4). Use an enum argument or separate routines. Overrides are left alone.", check: Check::Mixed(string_mode_parameter::string_mode_parameter),
        },
        Rule { id: "SV-DBC-001", name: "InterfaceWithoutAssertions", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &["MG-8.2"], experimental: false, native: false, about: "Flags an interface with clocking blocks or modports, so a protocol interface, that holds no assertions (assert/assume/cover property, immediate asserts, property declarations or checkers) and is not the target of a bind. Every bench that reuses it then re-checks the protocol by hand, or not at all (design by contract; MG-8.2). Advisory: off by default.", check: Check::Mixed(interface_without_assertions::interface_without_assertions),
        },
        Rule { id: "SV-DSN-011", name: "FeatureEnvy", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &["MG-3.1"], experimental: false, native: false, about: "Flags a method that uses one other object's fields at least 6 times (option min_foreign) and more than twice as often as its own class's fields. The logic belongs with the data it uses (Fowler: Feature Envy; MG-3.1). Transactions, config objects, the same class and classes in the same hierarchy do not count, nor do copy/compare/print and build/connect methods. Advisory: off by default.", check: Check::Mixed(feature_envy::feature_envy) },
        Rule { id: "SV-DUP-002", name: "CloneFragments", category: "Maintainability", severity: Severity::Warning, profiles: SMELLS, guidelines: &["MG-7.1"], experimental: false, native: false, about: "Finds runs of three or more statements (at least 50 tokens) that appear in more than one place, also when variables and literals were renamed consistently. Macro calls count as written, not as their expansion, and called names must match; runs that repeat one short cycle of statements, like unrolled loops, are left out. Each pair of routines is reported once; whole identical method bodies are SV-DUP-001's, and files whose header says they are generated are left out (DRY, Type-1/2 clones, MG-7.1).", check: Check::Mixed(clone_fragments::clone_fragments) },
        Rule { id: "SV-DUP-003", name: "DuplicatedConstraint", category: "Maintainability", severity: Severity::Warning, profiles: SMELLS, guidelines: &["MG-7.1"], experimental: false, native: false, about: "Flags a constraint body repeated in two classes that do not inherit from each other. The rule then lives in two places and drifts; declare it once in a shared base class or a policy object (Fowler 'pull up', MG-7.1). SV-RND-011 covers repeats inside one class.", check: Check::Mixed(duplicated_constraint::duplicated_constraint),
        },
        Rule { id: "SV-DUP-004", name: "RepeatedStringLiteral", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &["MG-7.1"], experimental: false, native: false, about: "Flags a string literal, such as a report ID or a file name, written four or more times in the code. Config keys (UVM-CFG-008), instance names and class names are left out. One named constant keeps the copies in step (DRY, MG-7.1). Advisory: off by default.", check: Check::Mixed(repeated_string_literal::repeated_string_literal),
        },
        Rule { id: "SV-DUP-005", name: "CopiedAgentPackage", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &["MG-7.2"], experimental: false, native: false, about: "Flags two packages whose classes have the same roles, fields and method bodies apart from renamed identifiers, for at least three classes and 80% of the classes in the larger package (classes with almost no code are not counted). They are one agent copied and renamed; a parameterized agent, or a shared base with small subclasses, keeps one copy to fix (DRY, MG-7.2). Advisory: off by default.", check: Check::Mixed(copied_agent_package::copied_agent_package),
        },
        Rule { id: "SV-EVO-001", name: "Hotspot", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &["MG-9.2"], experimental: false, native: false, about: "With --history: reports the files that are both complex (summed cyclomatic complexity of their routines) and often changed in git, the top five by complexity times commits. Such hotspots are where defects collect and where refactoring pays first (Tornhill, MG-9.2). Advisory: off by default.", check: Check::Mixed(hotspot::hotspot) },
        Rule { id: "SV-EVO-002", name: "ChangeCoupling", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &["MG-9.2"], experimental: false, native: false, about: "With --history: reports two files in different packages that change together in at least three commits and in at least half of the commits touching either. The packages have a hidden dependency that their imports do not show; move the shared knowledge to one place (temporal coupling, MG-9.2). Advisory: off by default.", check: Check::Mixed(change_coupling::change_coupling) },
        Rule { id: "SV-MET-009", name: "ConfigKnobBudget", category: "Advisory", severity: Severity::Warning, profiles: ADVISORY, guidelines: &[], experimental: false, native: false, about: "Flags a configuration object (a uvm_object named *cfg* or *config*) that declares more than max_knobs (default 12) data members of its own. Split it into per-agent or per-feature sub-configs that the parent config holds. Advisory: SV-MET-006 already allows config objects up to 80 fields, so this stricter budget is opt-in.", check: Check::Model(config_knob_budget::config_knob_budget),
        },
        Rule { id: "SV-PKG-001", name: "FileClassNameMismatch", category: "Architecture", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-1.2"], experimental: false, native: false, about: "Flags `include'd class files that define more than one top-level class, or whose one class is not named after the file (my_drv.svh holding my_driver). With one class per file, named after it, a class can be found from its name and reused on its own (MG-1.2).", check: Check::Mixed(file_class_name_mismatch::file_class_name_mismatch),
        },
        Rule { id: "SV-PKG-002", name: "WildcardImportOutsidePackage", category: "Architecture", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-1.3"], experimental: false, native: false, about: "Flags 'import pkg::*' at file ($unit) scope, and inside files that are `include'd into a package. Both leak names into code that never asked for them and hide where a dependency comes from (information hiding, MG-1.3).", check: Check::Mixed(wildcard_import_outside_package::wildcard_import_outside_package),
        },
        Rule { id: "SV-PKG-003", name: "UnusedPackageImport", category: "Architecture", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-1.3"], experimental: false, native: false, about: "Flags package imports that supply no name used in their scope. An unused import is a false dependency: the package is compiled first, changes to it look relevant, and names it exports can collide with local ones (common reuse principle, MG-1.3).", check: Check::Mixed(unused_package_import::unused_package_import),
        },
        Rule { id: "SV-PKG-004", name: "PackageLayerInversion", category: "Architecture", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-1.1"], experimental: false, native: false, about: "Flags a package that imports a package from a higher testbench layer, such as an agent package importing an env or test package. The lower package can then not be compiled or reused without the higher one (acyclic and stable dependencies, MG-1.1). UVM-ARC-001 checks the same order between classes.", check: Check::Mixed(package_layer_inversion::package_layer_inversion),
        },
        Rule { id: "SV-PKG-005", name: "UnstablePackageDependedOn", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &["MG-1.4"], experimental: false, native: false, about: "Computes instability I = Ce / (Ca + Ce) for each user package from its imports and flags a package that depends on a less stable one. Many packages rely on a stable package, so its own dependencies should be at least as stable (Martin's stable dependencies principle, MG-1.4). Advisory: off by default.", check: Check::Mixed(unstable_package_depended_on::unstable_package_depended_on),
        },
        Rule { id: "SV-RND-027", name: "SoftConstraintShadowing", category: "Randomization", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags soft constraints whose winner is hard to predict: 'soft' inside an inline randomize() with { } block, and a soft constraint on a variable that a base class or another constraint block of the same class already constrains softly. Keep soft defaults in one place, in the base class, and override with hard constraints.", check: Check::Mixed(soft_constraint_shadowing::soft_constraint_shadowing),
        },
        Rule { id: "UVM-CFG-007", name: "ConfigDbAsRuntimeChannel", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-2.3"], experimental: false, native: false, about: "Flags uvm_config_db get, set, exists and wait_modified calls in the run-time phases of components and inside loops of any task. config_db is build-time wiring; used at run time it becomes a slow global mailbox (common coupling, MG-2.3).", check: Check::Mixed(config_db_as_runtime_channel::config_db_as_runtime_channel),
        },
        Rule { id: "UVM-CFG-008", name: "ScatteredConfigKeyLiteral", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-2.4"], experimental: false, native: false, about: "Flags a uvm_config_db field name typed as a string literal in two or more files. The setter and getter only meet if both spell it the same; a shared constant makes the compiler check that (connascence of name, MG-2.4). Reported once per name.", check: Check::Mixed(scattered_config_key_literal::scattered_config_key_literal),
        },
        Rule { id: "UVM-CFG-009", name: "SharedConfigMutatedAtRuntime", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-2.3"], experimental: false, native: false, about: "Flags writes to fields of a config object that the class got from uvm_config_db and did not create, made outside new() and the build phases. The object is shared by everyone who read it, so the write is hidden global state (common coupling, MG-2.3).", check: Check::Mixed(shared_config_mutated_at_runtime::shared_config_mutated_at_runtime),
        },
        Rule { id: "UVM-CFG-010", name: "ConfigDbGetWithoutSet", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-2.4"], experimental: true, native: false, about: "Flags a uvm_config_db::get whose string field name has no set in this compilation, when the compilation contains both sets and gets. Experimental: a set in a file that was not linted, or a field name built at run time, is invisible here.", check: Check::Mixed(config_db_get_without_set::config_db_get_without_set),
        },
        Rule { id: "UVM-CFG-011", name: "ConfigDbSetNeverGot", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-2.4"], experimental: true, native: false, about: "Flags a uvm_config_db::set whose string field name has no get in this compilation, when the compilation contains both sets and gets. Experimental: a get in a file that was not linted, or a field name built at run time, is invisible here.", check: Check::Mixed(config_db_set_never_got::config_db_set_never_got),
        },
        Rule { id: "UVM-CPL-002", name: "HandleToEnvOrTest", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-2.1"], experimental: false, native: false, about: "Flags sequences, items, agents, config objects and helpers that store a handle to a user env or test class. They then depend on everything the env contains and cannot be used under another env (stamp coupling, dependency inversion, MG-2.1).", check: Check::Model(handle_to_env_or_test::handle_to_env_or_test),
        },
        Rule { id: "UVM-CPL-003", name: "ForeignComponentStateWrite", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-2.1"], experimental: false, native: false, about: "Flags assignments to properties of another component through a handle, except a parent setting up its own subtree in new() and the build, connect and end_of_elaboration phases (content coupling, MG-2.1). UVM-ARC-002 covers long read chains.", check: Check::Mixed(foreign_component_state_write::foreign_component_state_write),
        },
        Rule { id: "UVM-CPL-004", name: "HierarchicalReferenceInClass", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-2.2"], experimental: false, native: false, about: "Flags class code that names a module hierarchy (tb_top.dut.sig, $root.tb.x). Classes must reach the DUT only through virtual interfaces so they stay reusable (information hiding, MG-2.2).", check: Check::Mixed(hierarchical_reference_in_class::hierarchical_reference_in_class),
        },
        Rule { id: "UVM-PAT-001", name: "CallbackNotRegistered", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-5.1"], experimental: false, native: false, about: "Flags `uvm_do_callbacks(T, CB, ...) in classes where no `uvm_register_cb(T, CB) registers that pair for T or a base of T. UVM then refuses to add callbacks of that type at run time, so the hook never fires (MG-5.1).", check: Check::Mixed(callback_not_registered::callback_not_registered),
        },
        Rule { id: "UVM-PAT-002", name: "HandRolledSingleton", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-5.3"], experimental: false, native: false, about: "Flags a class that keeps a static variable of its own type, the usual hand-rolled singleton. Every env and test in the run shares that one object, and a test cannot replace it. Pass the object through uvm_config_db or the resource database (MG-5.3).", check: Check::Mixed(hand_rolled_singleton::hand_rolled_singleton),
        },
        Rule { id: "UVM-PAT-003", name: "NoExtensionPoint", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &["MG-5.2"], experimental: false, native: false, about: "Flags a driver or monitor whose run_phase is long (25 statements or more; option min_statements) while the class and its user bases declare no virtual hook methods and use no callbacks. A test or derived agent cannot then change one step, such as error injection or timing, without copying the whole run_phase (MG-5.2). Advisory: off by default.", check: Check::Mixed(no_extension_point::no_extension_point),
        },
        Rule { id: "UVM-PHS-011", name: "RunPhaseWithRuntimePhases", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Flags a component that overrides run_phase and also one of the runtime sub-phases (reset, configure, main, shutdown and their pre_/post_ phases). run_phase runs in parallel with all of them, so the two schedules race and readers cannot tell the order.", check: Check::Model(run_phase_with_runtime_phases::run_phase_with_runtime_phases),
        },
        Rule { id: "UVM-REP-007", name: "CheckFailureAsWarning", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-8.1"], experimental: false, native: false, about: "Flags a failed compare() or an expected-versus-actual mismatch reported only with `uvm_warning or `uvm_info. The test still passes; check failures must be errors so they are counted (MG-8.1).", check: Check::Mixed(check_failure_as_warning::check_failure_as_warning),
        },
        Rule { id: "UVM-REP-008", name: "FatalInCheckerPath", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-8.1"], experimental: false, native: false, about: "Flags `uvm_fatal on a data mismatch in scoreboards and subscribers. The run stops at the first mismatch and hides how many others there are; use `uvm_error and let max_quit_count stop the run (MG-8.1).", check: Check::Mixed(fatal_in_checker_path::fatal_in_checker_path),
        },
        Rule { id: "UVM-ROL-001", name: "DriverGeneratesStimulus", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-3.1"], experimental: false, native: false, about: "Flags drivers that randomize sequence items or start sequences. Stimulus belongs in sequences, where tests can layer constraints and the sequencer can arbitrate; a driver only converts items to pin activity (single responsibility, MG-3.1).", check: Check::Mixed(driver_generates_stimulus::driver_generates_stimulus),
        },
        Rule { id: "UVM-ROL-002", name: "CoverageOutsideObservers", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-3.1"], experimental: false, native: false, about: "Flags covergroups declared in drivers and sequences. They sample what was intended, not what reached the DUT, and disappear when the agent is passive; collect coverage in a monitor or subscriber (MG-3.1).", check: Check::Model(coverage_outside_observers::coverage_outside_observers),
        },
        Rule { id: "UVM-ROL-003", name: "DutCheckingInAgent", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-3.2"], experimental: false, native: false, about: "Flags agents that hold a scoreboard, predictor or reference model. Agents carry protocol knowledge only; DUT-specific checking belongs in the env so the agent can be reused with other DUTs (MG-3.2).", check: Check::Model(dut_checking_in_agent::dut_checking_in_agent),
        },
        Rule { id: "UVM-ROL-004", name: "TestBuildsLeafComponents", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-3.3"], experimental: false, native: false, about: "Flags tests that create drivers, monitors, sequencers, agents, scoreboards or subscribers directly. Tests configure and select; the env builds, so every test shares one structure (facade, MG-3.3).", check: Check::Mixed(test_builds_leaf_components::test_builds_leaf_components),
        },
        Rule { id: "UVM-ROL-005", name: "MonitorDependsOnActivePath", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-3.1"], experimental: false, native: false, about: "Flags monitors that hold a handle to a driver, sequencer or sequence item port. A monitor must work on its own, observing only the interface, or it breaks in passive agents (MG-3.1).", check: Check::Model(monitor_depends_on_active_path::monitor_depends_on_active_path),
        },
        Rule { id: "UVM-ROL-006", name: "VirtualInterfaceInChecker", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Flags virtual interface handles in scoreboards and subscribers. Pin timing belongs in drivers, monitors and interface assertions; a checker that waits on signals cannot run against a transaction-level model or an emulator, and hides a second monitor. Sequences are covered by UVM-SEQ-001.", check: Check::Model(virtual_interface_in_checker::virtual_interface_in_checker),
        },
        Rule { id: "UVM-RUS-001", name: "AgentIgnoresIsActive", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-4.1"], experimental: false, native: false, about: "Flags agents that create their driver or sequencer without checking is_active or get_is_active(). Such an agent cannot be reused as a passive monitor in a larger env (MG-4.1).", check: Check::Mixed(agent_ignores_is_active::agent_ignores_is_active),
        },
        Rule { id: "UVM-RUS-002", name: "AgentWithoutConfigObject", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-4.2"], experimental: false, native: false, about: "Flags agents and envs that read three or more separate scalar settings from uvm_config_db instead of one config object (Fowler 'data clumps', MG-4.2).", check: Check::Mixed(agent_without_config_object::agent_without_config_object),
        },
        Rule { id: "UVM-RUS-003", name: "HardcodedTopology", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-4.2"], experimental: false, native: false, about: "Flags envs that create components in a loop with a literal count instead of a count from their config object (open/closed principle, MG-4.2).", check: Check::Mixed(hardcoded_topology::hardcoded_topology),
        },
        Rule { id: "UVM-RUS-004", name: "AbsoluteConfigDbPath", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-4.3"], experimental: false, native: false, about: "Flags uvm_config_db calls in classes whose path starts at uvm_test_top. Below the test the path should be relative to 'this'; with a non-null context an absolute path is appended to the context and never matches (MG-4.3).", check: Check::Mixed(absolute_config_db_path::absolute_config_db_path),
        },
        Rule { id: "UVM-RUS-005", name: "OverrideByNameString", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-4.3"], experimental: false, native: false, about: "Flags factory overrides and creates that name types with string literals (set_type_override_by_name, set_inst_override, create_object ...). A typo or rename is only found at run time; T::type_id::set_type_override() is checked by the compiler (connascence of name, MG-4.3).", check: Check::Mixed(override_by_name_string::override_by_name_string),
        },
        Rule { id: "UVM-RUS-006", name: "IncompleteItemContract", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-4.4"], experimental: false, native: false, about: "Flags concrete sequence items with fields that implement neither field macros nor do_copy(), do_compare() and convert2string()/do_print(). Scoreboards, clone() and debug prints then silently see default behaviour (MG-4.4).", check: Check::Mixed(incomplete_item_contract::incomplete_item_contract),
        },
        Rule { id: "UVM-SEQ-016", name: "SequencerFromConfigDb", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Flags uvm_config_db/uvm_resource_db get() calls in a sequence that fetch a sequencer handle. Bind sub-sequencers once, in a virtual sequencer or the test.", check: Check::Mixed(sequencer_from_config_db::sequencer_from_config_db),
        },
    ]
}
