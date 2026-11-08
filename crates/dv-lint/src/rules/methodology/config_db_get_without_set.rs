//! UVM-CFG-010 (ConfigDbGetWithoutSet): Flags a uvm_config_db::get whose string
//! field name has no set in this compilation, when the compilation contains
//! both sets and gets.

use std::collections::HashSet;
use dv_model::Design;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::field_uses::{field_uses};

pub(crate) fn config_db_get_without_set(d: &Design, csts: &[Cst], out: &mut Sink) {
    let (sets, gets) = field_uses(d, csts);
    if sets.is_empty() || gets.is_empty() {
        return;
    }
    let names: HashSet<&str> = sets.iter().map(|s| s.0.as_str()).collect();
    for (f, loc) in &gets {
        if names.contains(f.as_str()) {
            continue;
        }
        out.add(
            "UVM-CFG-010",
            "ConfigDbGetWithoutSet",
            Severity::Warning,
            format!(
                "uvm_config_db::get of '{f}' has no uvm_config_db::set of that field name in this run. The get returns false and the target stays at its default. Set the \
field from the test or env, with the same type parameter."
            ),
            *loc,
        );
    }
}
