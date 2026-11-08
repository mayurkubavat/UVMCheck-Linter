//! SV-OOP-003 (MissingSuperNew): Constructors in derived classes should
//! explicitly invoke super.new(...) to ensure base class state is initialized.

use dv_model::Design;
use crate::{Severity, Sink, find_method};

pub(crate) fn missing_super_new(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        let Some(base) = cls.base else { continue };
        let Some(ctor) = find_method(cls, "new") else {
            continue;
        };
        if ctor.is_builtin || ctor.calls_super_new {
            continue;
        }
        out.add(
            "SV-OOP-003",
            "MissingSuperNew",
            Severity::Warning,
            format!(
                "Constructor 'new' in class '{}' derives from '{}' but does not explicitly call super.new(...).",
                cls.name,
                d.class(base).name
            ),
            ctor.loc,
        );
    }
}
