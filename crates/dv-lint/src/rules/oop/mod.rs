//! The oop rules: the Rust twins of `cpp/src/rules/oop/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::profile::*;
use crate::{Check, Rule, Severity};

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
        Rule { id: "SV-DUP-001", name: "DuplicateMethodBody", category: "Maintainability", severity: Severity::Warning, profiles: SMELLS, guidelines: &["MG-7.1"], experimental: false, native: false, about: "Detects identical or copy-pasted subroutine method bodies across different classes. Duplicated code should be refactored into shared base classes or utilities.", check: Check::Mixed(duplicate_method_body::duplicate_body),
        },
        Rule { id: "SV-ENC-001", name: "MutableObjectGetterLeak", category: "OOP", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-2.1"], experimental: false, native: false, about: "Disallows getter functions (get_*()) from returning direct references to mutable internal class object members. Returning mutable handles leaks encapsulation; return a clone or copy instead.", check: Check::Syntax(mutable_object_getter_leak::getter_leak),
        },
        Rule { id: "SV-LSP-001", name: "LspConstraintContradiction", category: "OOP", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &["MG-5.2"], experimental: false, native: false, about: "Disallows unconditionally disabling inherited constraints via '.constraint_mode(0)' in derived classes. Disabling inherited constraints breaks base class contracts and violates the Liskov Substitution Principle (LSP).", check: Check::Syntax(lsp_constraint_contradiction::lsp_constraint_mode),
        },
        Rule { id: "SV-OCP-001", name: "CastTypeInspectionLadder", category: "OOP", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-5.2"], experimental: false, native: false, about: "Disallows cascading $cast ladders (3+ branches) inspecting object types. Type inspection ladders violate the Open-Closed Principle (OCP); use virtual polymorphic methods instead.", check: Check::Syntax(cast_type_inspection_ladder::cast_ladder),
        },
        Rule { id: "SV-OOP-001", name: "NonVirtualMethodOverride", category: "OOP", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-5.2"], experimental: false, native: false, about: "Overriding a non-virtual method in a derived class breaks polymorphism when accessed via a base class handle.", check: Check::Model(non_virtual_method_override::non_virtual_method_override),
        },
        Rule { id: "SV-OOP-002", name: "UncheckedDynamicCast", category: "OOP", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-8.1"], experimental: false, native: false, about: "The return value of $cast(dest, src) must be checked (e.g., in an if statement or assert) to prevent silent null-pointer dereferences on cast failures.", check: Check::Syntax(unchecked_dynamic_cast::unchecked_dynamic_cast),
        },
        Rule { id: "SV-OOP-003", name: "MissingSuperNew", category: "OOP", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Constructors in derived classes should explicitly invoke super.new(...) to ensure base class state is initialized.", check: Check::Model(missing_super_new::missing_super_new) },
        Rule { id: "SV-OOP-004", name: "VariableShadowing", category: "OOP", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Detects variable shadowing where a derived class property shadows a base class property, or a method argument / local variable shadows a class member property.", check: Check::Model(variable_shadowing::variable_shadowing),
        },
        Rule { id: "SV-OOP-005", name: "VirtualCallInConstructor", category: "OOP", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Never call virtual methods inside constructors. In SystemVerilog, base constructors execute before child members are initialized, resulting in child methods operating on uninitialized data.", check: Check::Model(virtual_call_in_constructor::virtual_call_in_constructor),
        },
        Rule { id: "SV-OOP-006", name: "ClassShallowCopyAliasing", category: "OOP", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Using shallow copy (new other_obj) on classes containing nested object handles creates shared references across independent transactions. Use deep copy or clone() instead.", check: Check::Syntax(class_shallow_copy_aliasing::shallow_copy),
        },
        Rule { id: "SV-OOP-007", name: "StaticMemberInUvmComponent", category: "Architecture", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &["MG-2.3"], experimental: false, native: false, about: "Non-const static variable declared in a uvm_component subclass is shared across all instances simulation-wide. In multi-channel or multi-agent environments, instances will overwrite each other's state.", check: Check::Model(static_member_in_uvm_component::static_member_in_uvm_component),
        },
        Rule { id: "SV-OOP-008", name: "NonVirtualInterfaceInClass", category: "OOP", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &["MG-2.2"], experimental: false, native: false, about: "Disallows declaring physical interface types inside classes without the 'virtual' keyword (IEEE 1800 § 25.9).", check: Check::Syntax(non_virtual_interface_in_class::non_virtual_interface),
        },
        Rule { id: "SV-OOP-009", name: "EmptyVirtualMethodWithoutPure", category: "OOP", severity: Severity::Info, profiles: METHODOLOGY, guidelines: &["MG-5.2"], experimental: false, native: false, about: "Flags virtual methods defined with empty bodies instead of being declared 'pure virtual' in abstract base classes.", check: Check::Syntax(empty_virtual_method_without_pure::empty_virtual),
        },
        Rule { id: "SV-OOP-011", name: "DowncastWithoutDynamicCast", category: "OOP", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Flags static casting syntax (e.g. DerivedClass'(base_handle)) applied to class objects. SystemVerilog requires dynamic casting via $cast() to verify runtime polymorphism.", check: Check::Mixed(downcast_without_dynamic_cast::downcast),
        },
        Rule { id: "SV-OOP-012", name: "ChainedAssignmentAliasing", category: "OOP", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Disallows chained assignment expressions (e.g. a = b = c;). Chained assignments create dangerous reference aliasing across class handles.", check: Check::Syntax(chained_assignment_aliasing::chained_assignment),
        },
        Rule { id: "SV-OOP-013", name: "AbstractClassInstantiation", category: "OOP", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Disallows direct instantiation of abstract (virtual) classes via 'new()'. Only concrete derived classes can be instantiated.", check: Check::Syntax(abstract_class_instantiation::abstract_new),
        },
        Rule { id: "SV-OOP-014", name: "SuperNewNotFirstStatement", category: "OOP", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Derived class constructors calling super.new(...) must invoke it as the very first procedural statement.", check: Check::Model(super_new_not_first_statement::super_new_not_first_statement),
        },
        Rule { id: "SV-OOP-015", name: "VirtualMethodSignatureMismatch", category: "OOP", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &["MG-5.2"], experimental: false, native: false, about: "Virtual method overrides in derived classes must match the parameter count and signature of the base virtual method.", check: Check::Model(virtual_method_signature_mismatch::virtual_method_signature_mismatch),
        },
        Rule { id: "SV-SHD-002", name: "ForeachLoopVariableShadowing", category: "OOP", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Flags loop index variables in 'foreach (arr[i])' loops that shadow an existing class property or routine local variable.", check: Check::Syntax(foreach_loop_variable_shadowing::foreach_shadowing),
        },
    ]
}
