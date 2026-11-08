//! UVM-FAC-005 (GlobalFactoryOverridePollution): Flags global factory type
//! overrides (set_type_override, set_type_override_by_type,
//! factory.set_type_override_by_name, T::type_id::set_type_override) outside
//! uvm_test classes.

use crate::support::uvm_classes::{class_role, user_classes, Role};
use crate::{Severity, Sink};
use dv_model::Design;

fn role_name(r: Role) -> &'static str {
    match r {
        Role::None => "class",
        Role::Object => "object",
        Role::Config => "config",
        Role::Item => "sequence item",
        Role::Sequence => "sequence",
        Role::Component => "component",
        Role::Driver => "driver",
        Role::Monitor => "monitor",
        Role::Sequencer => "sequencer",
        Role::Agent => "agent",
        Role::Scoreboard => "scoreboard",
        Role::Subscriber => "subscriber",
        Role::Env => "env",
        Role::Test => "test",
        Role::RegModel => "register model",
        Role::Catcher => "report catcher",
        Role::Callback => "callback",
    }
}

pub(crate) fn global_factory_override(d: &Design, s: &mut Sink) {
    for cls in user_classes(d) {
        let role = class_role(d, cls.id);
        if role == Role::Test || role == Role::None {
            continue;
        }
        for m in &cls.methods {
            if m.is_builtin || m.is_pure {
                continue;
            }
            for k in &m.calls {
                let n = k.name.as_str();
                let owner = k.callee_scope.as_str();
                let global = match owner {
                    "uvm_component" => n == "set_type_override" || n == "set_type_override_by_type",
                    "uvm_factory" | "uvm_default_factory" => {
                        n == "set_type_override_by_type" || n == "set_type_override_by_name"
                    }
                    "uvm_component_registry"
                    | "uvm_object_registry"
                    | "uvm_abstract_component_registry"
                    | "uvm_abstract_object_registry" => n == "set_type_override",
                    _ => false,
                };
                if !global || k.is_system {
                    continue;
                }
                s.add(
                    "UVM-FAC-005",
                    "GlobalFactoryOverridePollution",
                    Severity::Warning,
                    format!(
                        "{n}() in {} '{}' installs a global type override, which changes this type for every \
                         test and VIP in the simulation. Leave type overrides to the test, or use \
                         an instance override (set_inst_override_by_type) scoped to this \
                         component's sub-tree.",
                        role_name(role),
                        cls.name
                    ),
                    k.loc,
                );
            }
        }
    }
}
