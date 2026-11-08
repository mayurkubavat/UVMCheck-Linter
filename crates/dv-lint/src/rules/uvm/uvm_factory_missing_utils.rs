//! UVM-FAC-003 (UvmFactoryMissingUtils): Concrete classes derived from
//! uvm_object or uvm_component must declare factory registration macros
//! (`uvm_component_utils / `uvm_object_utils).

use dv_model::Design;
use crate::{find_method, find_property, Severity, Sink};
use crate::support::uvm_kinds::{is_component, is_object};

pub(crate) fn uvm_factory_missing_utils(d: &Design, out: &mut Sink) {
    const REGISTERS: [&str; 4] =
        ["uvm_component_utils", "uvm_object_utils", "uvm_component_param_utils", "uvm_object_param_utils"];
    for cls in d.classes() {
        if cls.is_parameterized || cls.is_virtual || cls.is_interface || cls.name.starts_with("uvm_") {
            continue;
        }
        let (comp, obj) = (is_component(d, cls), is_object(d, cls));
        if !comp && !obj {
            continue;
        }
        // A member of its own named type_id or get_type, or a registration macro.
        let declares = |n: &str| {
            cls.typedefs.iter().any(|t| t.name == n) || find_property(cls, n).is_some() || find_method(cls, n).is_some()
        };
        if declares("type_id") || declares("get_type") {
            continue;
        }
        if cls.macros.iter().any(|m| REGISTERS.iter().any(|r| m.name.starts_with(r))) {
            continue;
        }
        let (kind, mac) = if comp { ("uvm_component", "`uvm_component_utils") } else { ("uvm_object", "`uvm_object_utils") };
        out.add(
            "UVM-FAC-003",
            "UvmFactoryMissingUtils",
            Severity::Warning,
            format!(
                "Concrete class '{0}' derived from '{kind}' is not registered with the UVM factory. Add '{mac}({0})' to enable testbench factory overrides.",
                cls.name
            ),
            cls.loc,
        );
    }
}
