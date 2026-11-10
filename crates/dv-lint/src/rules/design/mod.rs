//! The design rules: the Rust twins of `cpp/src/rules/design/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::profile::*;
use crate::{Check, Rule, Severity};

mod data_clumps;
mod deep_inheritance;
mod empty_override_of_concrete;
mod fatal_stub_override;
mod file_included_in_multiple_packages;
mod god_class;
mod hierarchy_reach_through;
mod high_coupling;
mod layer_dependency;
mod low_cohesion;
mod mutable_package_state;
mod never_set_class_parameter;
mod override_only_calls_super;
mod plusargs_outside_test;
mod primitive_obsession;
mod procedural_timeout_in_monitor;
mod query_changes_state;
mod repeated_enum_switch;
mod string_hierarchy_lookup;
mod type_switch_by_name;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-DED-015", name: "NeverSetClassParameter", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &[], experimental: false, native: false, about: "Flags a parameter of a user parameterized class that no specialization in the linted design ever sets. The class claims a flexibility nobody uses or verifies (Fowler 'speculative generality'); make it a localparam or drop it. Whole-program: a specialization in a file that was not linted is not seen, and only specializations used as a class member or base class are counted. Advisory: off by default.", check: Check::Syntax(never_set_class_parameter::never_set_parameter),
        },
        Rule { id: "SV-DSN-001", name: "TypeSwitchByName", category: "Architecture", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-5.2"], experimental: false, native: false, about: "Flags branching on get_type_name() compared with string literals instead of virtual dispatch (Sonar 'instanceof chains', open/closed principle).", check: Check::Syntax(type_switch_by_name::type_switch_by_name),
        },
        Rule { id: "SV-DSN-002", name: "GodClass", category: "Maintainability", severity: Severity::Warning, profiles: SMELLS, guidelines: &["MG-3.1"], experimental: false, native: false, about: "Flags classes that are very complex (WMC >= 47), use many other classes' data (ATFD > 5) and have low cohesion (TCC < 1/3): PMD's GodClass detection strategy.", check: Check::Syntax(god_class::god_class) },
        Rule { id: "SV-DSN-003", name: "LowCohesion", category: "Maintainability", severity: Severity::Info, profiles: SMELLS, guidelines: &["MG-3.1"], experimental: false, native: false, about: "Flags classes whose methods split into two or more groups that share no fields and do not call each other (LCOM4 > 1): the class is really several classes.", check: Check::Syntax(low_cohesion::low_cohesion) },
        Rule { id: "SV-DSN-004", name: "HighCoupling", category: "Maintainability", severity: Severity::Info, profiles: SMELLS, guidelines: &["MG-2.1"], experimental: false, native: false, about: "Flags classes other than envs and tests that reference more than 10 distinct user class types (CK coupling between objects, PMD CouplingBetweenObjects).", check: Check::Syntax(high_coupling::high_coupling) },
        Rule { id: "SV-DSN-005", name: "DeepInheritance", category: "Maintainability", severity: Severity::Info, profiles: SMELLS, guidelines: &[], experimental: false, native: false, about: "Flags classes with more than 3 user-defined ancestors above the UVM base class (CK depth of inheritance tree, Pylint too-many-ancestors).", check: Check::Syntax(deep_inheritance::deep_inheritance) },
        Rule { id: "SV-DSN-006", name: "OverrideOnlyCallsSuper", category: "Maintainability", severity: Severity::Info, profiles: SMELLS, guidelines: &["MG-5.2"], experimental: false, native: false, about: "Flags overrides whose whole body is the super call with the same arguments, such as a connect_phase that only calls super.connect_phase(phase) (Pylint useless-parent-delegation, Sonar S1185). Constructors are kept.", check: Check::Syntax(override_only_calls_super::override_only_calls_super),
        },
        Rule { id: "SV-DSN-007", name: "EmptyOverrideOfConcrete", category: "Maintainability", severity: Severity::Warning, profiles: SMELLS, guidelines: &["MG-5.2"], experimental: false, native: false, about: "Flags an empty override of a user base-class method that has a body, which silently switches the inherited behavior off (Fowler 'refused bequest', Liskov substitution).", check: Check::Syntax(empty_override_of_concrete::empty_override),
        },
        Rule { id: "SV-DSN-008", name: "MutablePackageState", category: "Architecture", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-2.3"], experimental: false, native: false, about: "Flags non-const variables declared in packages. They are shared by every env instance and test in a run (clang-tidy cppcoreguidelines-avoid-non-const-global-variables).", check: Check::Syntax(mutable_package_state::mutable_package_state),
        },
        Rule { id: "SV-DSN-009", name: "RepeatedEnumSwitch", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &["MG-5.2"], experimental: false, native: false, about: "Flags an enum that is switched on with case in three or more classes, a sign that behavior per kind belongs in a polymorphic item or policy class (Fowler 'repeated switches'). Advisory: off by default.", check: Check::Syntax(repeated_enum_switch::repeated_enum_switch),
        },
        Rule { id: "SV-DSN-010", name: "QueryChangesState", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &[], experimental: false, native: false, about: "Flags functions named get_*, is_* or has_* that write class properties (command-query separation). get_next_* is allowed. Advisory: off by default.", check: Check::Syntax(query_changes_state::query_changes_state),
        },
        Rule { id: "SV-DSN-012", name: "PrimitiveObsession", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &[], experimental: false, native: false, about: "Flags a rand integral field of a sequence item or config that a constraint limits to a list of 3 to 16 single constant values ('x inside {0, 1, 2}'). The field is an enumeration: as an enum it prints by name in logs and waveforms and the compiler checks every value. Advisory: off by default.", check: Check::Syntax(primitive_obsession::primitive_obsession),
        },
        Rule { id: "SV-DSN-013", name: "FatalStubOverride", category: "Maintainability", severity: Severity::Warning, profiles: SMELLS, guidelines: &[], experimental: false, native: false, about: "Flags an override of a user base-class method whose whole body is an error or fatal report (\"not supported here\"). The subclass refuses part of its parent's contract, so code holding a base handle fails at run time (Fowler 'refused bequest'). Split the base class instead. SV-DSN-007 covers empty overrides.", check: Check::Syntax(fatal_stub_override::fatal_stub_override),
        },
        Rule { id: "SV-DUP-006", name: "DataClumps", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &[], experimental: false, native: false, about: "Flags the same group of min_args (default 4) or more arguments (same names and types) declared by min_routines (default 3) or more routines. Values that always travel together belong in a transaction or struct; passing them one by one lets two same-typed arguments be swapped silently (Fowler 'data clumps'). Overrides are counted once, at the base declaration; each group is reported once, at its first routine. Advisory: off by default.", check: Check::Syntax(data_clumps::data_clumps) },
        Rule { id: "UVM-ARC-001", name: "LayerDependency", category: "Architecture", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-1.1"], experimental: false, native: false, about: "Flags classes that depend on a higher testbench layer (item < sequence < agent < env < test), such as a sequence that references a scoreboard or an agent that references the env (ArchUnit layer rules).", check: Check::Syntax(layer_dependency::layer_dependency) },
        Rule { id: "UVM-ARC-002", name: "HierarchyReachThrough", category: "Architecture", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-2.1"], experimental: false, native: false, about: "Flags expressions that reach through three or more component handles (env.agt.drv.vif), outside connect_phase wiring (PMD LawOfDemeter, Fowler 'message chains').", check: Check::Syntax(hierarchy_reach_through::hierarchy_reach_through),
        },
        Rule { id: "UVM-ARC-003", name: "FileIncludedInMultiplePackages", category: "Architecture", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &["MG-1.3"], experimental: false, native: false, about: "Flags a file whose class definitions are `include'd into more than one package. Each package gets its own, incompatible type with the same name (C++ one-definition rule).", check: Check::Syntax(file_included_in_multiple_packages::file_in_many_packages),
        },
        Rule { id: "UVM-ARC-004", name: "StringHierarchyLookup", category: "Architecture", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-2.3"], experimental: false, native: false, about: "Flags uvm_root/uvm_component find(), find_all() and lookup() calls outside tests and end_of_elaboration_phase. Looking components up by path string is a service locator that hides dependencies and breaks silently when the hierarchy changes.", check: Check::Syntax(string_hierarchy_lookup::string_hierarchy_lookup),
        },
        Rule { id: "UVM-CFG-006", name: "PlusargsOutsideTest", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-2.3"], experimental: false, native: false, about: "Flags $test$plusargs, $value$plusargs and uvm_cmdline_processor reads in classes other than tests and config objects. Reading the command line deep inside components is hidden global configuration.", check: Check::Syntax(plusargs_outside_test::plusargs_outside_test),
        },
        Rule { id: "UVM-ROL-007", name: "ProceduralTimeoutInMonitor", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &[], experimental: false, native: false, about: "Flags fork ... join_any blocks in a monitor where one branch counts clock cycles (repeat (N) @(...)) and then reports an error: a handshake timeout written as procedural code. A concurrent assertion in the interface states the same rule in one line and checks it every cycle. Advisory: off by default.", check: Check::Syntax(procedural_timeout_in_monitor::procedural_timeout),
        },
    ]
}
