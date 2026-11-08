//! The design rules: the Rust twins of `cpp/src/rules/design/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

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
        Rule {
            id: "SV-DED-015",
            name: "NeverSetClassParameter",
            check: Check::Syntax(never_set_class_parameter::never_set_parameter),
        },
        Rule {
            id: "SV-DSN-001",
            name: "TypeSwitchByName",
            check: Check::Syntax(type_switch_by_name::type_switch_by_name),
        },
        Rule { id: "SV-DSN-002", name: "GodClass", check: Check::Syntax(god_class::god_class) },
        Rule { id: "SV-DSN-003", name: "LowCohesion", check: Check::Syntax(low_cohesion::low_cohesion) },
        Rule { id: "SV-DSN-004", name: "HighCoupling", check: Check::Syntax(high_coupling::high_coupling) },
        Rule { id: "SV-DSN-005", name: "DeepInheritance", check: Check::Syntax(deep_inheritance::deep_inheritance) },
        Rule {
            id: "SV-DSN-006",
            name: "OverrideOnlyCallsSuper",
            check: Check::Syntax(override_only_calls_super::override_only_calls_super),
        },
        Rule {
            id: "SV-DSN-007",
            name: "EmptyOverrideOfConcrete",
            check: Check::Syntax(empty_override_of_concrete::empty_override),
        },
        Rule {
            id: "SV-DSN-008",
            name: "MutablePackageState",
            check: Check::Syntax(mutable_package_state::mutable_package_state),
        },
        Rule {
            id: "SV-DSN-009",
            name: "RepeatedEnumSwitch",
            check: Check::Syntax(repeated_enum_switch::repeated_enum_switch),
        },
        Rule {
            id: "SV-DSN-010",
            name: "QueryChangesState",
            check: Check::Syntax(query_changes_state::query_changes_state),
        },
        Rule {
            id: "SV-DSN-012",
            name: "PrimitiveObsession",
            check: Check::Syntax(primitive_obsession::primitive_obsession),
        },
        Rule {
            id: "SV-DSN-013",
            name: "FatalStubOverride",
            check: Check::Syntax(fatal_stub_override::fatal_stub_override),
        },
        Rule { id: "SV-DUP-006", name: "DataClumps", check: Check::Syntax(data_clumps::data_clumps) },
        Rule { id: "UVM-ARC-001", name: "LayerDependency", check: Check::Syntax(layer_dependency::layer_dependency) },
        Rule {
            id: "UVM-ARC-002",
            name: "HierarchyReachThrough",
            check: Check::Syntax(hierarchy_reach_through::hierarchy_reach_through),
        },
        Rule {
            id: "UVM-ARC-003",
            name: "FileIncludedInMultiplePackages",
            check: Check::Syntax(file_included_in_multiple_packages::file_in_many_packages),
        },
        Rule {
            id: "UVM-ARC-004",
            name: "StringHierarchyLookup",
            check: Check::Syntax(string_hierarchy_lookup::string_hierarchy_lookup),
        },
        Rule {
            id: "UVM-CFG-006",
            name: "PlusargsOutsideTest",
            check: Check::Syntax(plusargs_outside_test::plusargs_outside_test),
        },
        Rule {
            id: "UVM-ROL-007",
            name: "ProceduralTimeoutInMonitor",
            check: Check::Syntax(procedural_timeout_in_monitor::procedural_timeout),
        },
    ]
}
