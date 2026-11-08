//! UVM-STY-002 (ConstraintNameSuffix): Enforces a naming standard requiring all
//! constraint blocks to end with '_c' suffix.

use crate::{Severity, Sink};
use dv_model::Design;

pub(crate) fn constraint_name_suffix(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        for c in &cls.constraints {
            if c.name.ends_with("_c") {
                continue;
            }
            out.add(
                "UVM-STY-002",
                "ConstraintNameSuffix",
                Severity::Warning,
                format!(
                    "Constraint block '{}' in class '{}' does not follow the constraint naming convention. \
Constraint block names must end with '_c' suffix (e.g. '{}_c') to clearly distinguish constraint sets from random variables in solver logs.",
                    c.name, cls.name, c.name
                ),
                c.loc,
            );
        }
    }
}
