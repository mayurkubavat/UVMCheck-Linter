//! UVM-CFG-011 (ConfigDbSetNeverGot): Flags a uvm_config_db::set whose string
//! field name has no get in this compilation, when the compilation contains
//! both sets and gets.

use std::collections::HashSet;
use dv_model::Design;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::field_uses::{field_uses};

pub(crate) fn config_db_set_never_got(d: &Design, csts: &[Cst], out: &mut Sink) {
    let (sets, gets) = field_uses(d, csts);
    if sets.is_empty() || gets.is_empty() {
        return;
    }
    let names: HashSet<&str> = gets.iter().map(|s| s.0.as_str()).collect();
    for (f, loc) in &sets {
        if names.contains(f.as_str()) {
            continue;
        }
        out.add(
            "UVM-CFG-011",
            "ConfigDbSetNeverGot",
            Severity::Warning,
            format!(
                "uvm_config_db::set of '{f}' has no uvm_config_db::get of that field name in this run. The value is stored and never read. Remove the set, or get it where \
the component is built."
            ),
            *loc,
        );
    }
}
