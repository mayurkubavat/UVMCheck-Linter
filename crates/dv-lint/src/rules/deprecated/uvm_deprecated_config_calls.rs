//! UVM-DEP-001 (UvmDeprecatedConfigCalls): Disallow deprecated legacy
//! set_config_* and get_config_* methods (IEEE 1800.2 Annex H).

use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::copies::{copies, routine_calls};

const CONFIG_CALLS: [&str; 6] = [
    "set_config_int",
    "set_config_string",
    "set_config_object",
    "get_config_int",
    "get_config_string",
    "get_config_object",
];

pub(crate) fn deprecated_config_calls(c: &Cst, s: &mut Sink) {
    for (n, name, first) in routine_calls(c, &|x| CONFIG_CALLS.contains(&x)) {
        for _ in 0..copies(c, n) {
            s.add(
                "UVM-DEP-001",
                "UvmDeprecatedConfigCalls",
                Severity::Error,
                format!(
                    "Legacy configuration method '{name}' is formally deprecated in IEEE 1800.2 (Annex H). \
                     These string-based methods lack type safety and break under +define+UVM_NO_DEPRECATED. \
                     Replace with modern, type-safe 'uvm_config_db#(T)::set(this, ...)' and 'uvm_config_db#(T)::get(this, ...)'."
                ),
                c.loc(first),
            );
        }
    }
}
