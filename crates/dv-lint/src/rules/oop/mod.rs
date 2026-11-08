//! The oop rules: the Rust twins of `cpp/src/rules/oop/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

mod abstract_class_instantiation;
mod cast_type_inspection_ladder;
mod chained_assignment_aliasing;
mod class_shallow_copy_aliasing;
mod downcast_without_dynamic_cast;
mod duplicate_method_body;
mod empty_virtual_method_without_pure;
mod foreach_loop_variable_shadowing;
mod lsp_constraint_contradiction;
mod missing_super_new;
mod mutable_object_getter_leak;
mod non_virtual_interface_in_class;
mod non_virtual_method_override;
mod static_member_in_uvm_component;
mod super_new_not_first_statement;
mod unchecked_dynamic_cast;
mod variable_shadowing;
mod virtual_call_in_constructor;
mod virtual_method_signature_mismatch;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-DUP-001", name: "DuplicateMethodBody", check: Check::Mixed(duplicate_method_body::duplicate_body) },
        Rule { id: "SV-ENC-001", name: "MutableObjectGetterLeak", check: Check::Syntax(mutable_object_getter_leak::getter_leak) },
        Rule { id: "SV-LSP-001", name: "LspConstraintContradiction", check: Check::Syntax(lsp_constraint_contradiction::lsp_constraint_mode) },
        Rule { id: "SV-OCP-001", name: "CastTypeInspectionLadder", check: Check::Syntax(cast_type_inspection_ladder::cast_ladder) },
        Rule { id: "SV-OOP-001", name: "NonVirtualMethodOverride", check: Check::Model(non_virtual_method_override::non_virtual_method_override) },
        Rule { id: "SV-OOP-002", name: "UncheckedDynamicCast", check: Check::Syntax(unchecked_dynamic_cast::unchecked_dynamic_cast) },
        Rule { id: "SV-OOP-003", name: "MissingSuperNew", check: Check::Model(missing_super_new::missing_super_new) },
        Rule { id: "SV-OOP-004", name: "VariableShadowing", check: Check::Model(variable_shadowing::variable_shadowing) },
        Rule { id: "SV-OOP-005", name: "VirtualCallInConstructor", check: Check::Model(virtual_call_in_constructor::virtual_call_in_constructor) },
        Rule { id: "SV-OOP-006", name: "ClassShallowCopyAliasing", check: Check::Syntax(class_shallow_copy_aliasing::shallow_copy) },
        Rule { id: "SV-OOP-007", name: "StaticMemberInUvmComponent", check: Check::Model(static_member_in_uvm_component::static_member_in_uvm_component) },
        Rule { id: "SV-OOP-008", name: "NonVirtualInterfaceInClass", check: Check::Syntax(non_virtual_interface_in_class::non_virtual_interface) },
        Rule { id: "SV-OOP-009", name: "EmptyVirtualMethodWithoutPure", check: Check::Syntax(empty_virtual_method_without_pure::empty_virtual) },
        Rule { id: "SV-OOP-011", name: "DowncastWithoutDynamicCast", check: Check::Mixed(downcast_without_dynamic_cast::downcast) },
        Rule { id: "SV-OOP-012", name: "ChainedAssignmentAliasing", check: Check::Syntax(chained_assignment_aliasing::chained_assignment) },
        Rule { id: "SV-OOP-013", name: "AbstractClassInstantiation", check: Check::Syntax(abstract_class_instantiation::abstract_new) },
        Rule { id: "SV-OOP-014", name: "SuperNewNotFirstStatement", check: Check::Model(super_new_not_first_statement::super_new_not_first_statement) },
        Rule { id: "SV-OOP-015", name: "VirtualMethodSignatureMismatch", check: Check::Model(virtual_method_signature_mismatch::virtual_method_signature_mismatch) },
        Rule { id: "SV-SHD-002", name: "ForeachLoopVariableShadowing", check: Check::Syntax(foreach_loop_variable_shadowing::foreach_shadowing) },
    ]
}
