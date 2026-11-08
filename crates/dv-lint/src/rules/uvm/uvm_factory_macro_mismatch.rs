//! UVM-FAC-002 (UvmFactoryMacroMismatch): Classes derived from uvm_component
//! must use `uvm_component_utils, and classes derived from uvm_object
//! (non-component) must use `uvm_object_utils.

use dv_model::Design;
use crate::{Severity, Sink};
use crate::support::uvm_kinds::{is_component, is_object};

pub(crate) fn uvm_factory_macro_mismatch(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        let (comp, obj) = (is_component(d, cls), is_object(d, cls));
        if !comp && !obj {
            continue;
        }
        for m in &cls.macros {
            if comp && m.name.starts_with("uvm_object_utils") {
                out.add(
                    "UVM-FAC-002",
                    "UvmFactoryMacroMismatch",
                    Severity::Error,
                    format!(
                        "Class '{}' derives from 'uvm_component' but declares '`{}'. Use '`uvm_component_utils' \
for components to support hierarchical factory registration.",
                        cls.name, m.name
                    ),
                    m.loc,
                );
            } else if !comp && obj && m.name.starts_with("uvm_component_utils") {
                out.add(
                    "UVM-FAC-002",
                    "UvmFactoryMacroMismatch",
                    Severity::Error,
                    format!(
                        "Class '{}' derives from 'uvm_object' (not a component) but declares '`{}'. Use '`uvm_object_utils' \
because non-component objects do not have parent hierarchy contexts.",
                        cls.name, m.name
                    ),
                    m.loc,
                );
            }
        }
        // The C++ rule reads the registry from how slang prints the type_id
        // alias, which is the alias's own path (pkg::cls::type_id).
        for t in cls.typedefs.iter().filter(|t| t.name == "type_id") {
            let printed = if cls.scope.is_empty() {
                format!("{}::{}", cls.name, t.name)
            } else {
                format!("{}::{}::{}", cls.scope, cls.name, t.name)
            };
            if comp && printed.contains("uvm_object_registry") {
                out.add(
                    "UVM-FAC-002",
                    "UvmFactoryMacroMismatch",
                    Severity::Error,
                    format!(
                        "Class '{}' derives from 'uvm_component' but its factory registry is 'uvm_object_registry'. Use 'uvm_component_registry'.",
                        cls.name
                    ),
                    t.loc,
                );
            } else if !comp && obj && printed.contains("uvm_component_registry") {
                out.add(
                    "UVM-FAC-002",
                    "UvmFactoryMacroMismatch",
                    Severity::Error,
                    format!(
                        "Class '{}' derives from 'uvm_object' but its factory registry is 'uvm_component_registry'. Use 'uvm_object_registry'.",
                        cls.name
                    ),
                    t.loc,
                );
            }
        }
    }
}
