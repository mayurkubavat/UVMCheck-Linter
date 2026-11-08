//! UVM-CFG-002 (UvmConfigDbNullContext): Passing 'null' as context to
//! uvm_config_db inside a uvm_component breaks hierarchical scoping and
//! prevents targeted configuration overrides.

use crate::{Severity, Sink};
use dv_model::Design;

pub(crate) fn uvm_config_db_null_context(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || !cls.uvm.is_component() {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.is_builtin {
                continue;
            }
            for c in &sub.calls {
                if c.is_system || (c.name != "get" && c.name != "set") {
                    continue;
                }
                if !c.callee_scope.contains("uvm_config_db") || !c.first_arg_is_null {
                    continue;
                }
                out.add(
                    "UVM-CFG-002",
                    "UvmConfigDbNullContext",
                    Severity::Warning,
                    format!(
                        "Passing 'null' as context to uvm_config_db::{} inside uvm_component '{}'. Pass 'this' instead to preserve hierarchical scoping.",
                        c.name, cls.name
                    ),
                    c.first_arg_loc,
                );
            }
        }
    }
}
