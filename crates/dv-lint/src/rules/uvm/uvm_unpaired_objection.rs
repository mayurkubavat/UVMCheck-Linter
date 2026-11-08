//! UVM-OBJ-001 (UvmUnpairedObjection): Every phase.raise_objection() call must
//! have a corresponding phase.drop_objection() within the same execution
//! context to prevent simulation hangs or fatal errors.

use crate::{Severity, Sink};
use dv_model::{Design, UvmKind};

pub(crate) fn uvm_unpaired_objection(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || (!cls.uvm.is_component() && cls.uvm != UvmKind::Sequence) {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.is_builtin {
                continue;
            }
            let raise = sub.calls.iter().find(|c| c.name == "raise_objection");
            let drop = sub.calls.iter().find(|c| c.name == "drop_objection");
            match (raise, drop) {
                (Some(r), None) => {
                    out.add(
                        "UVM-OBJ-001",
                        "UvmUnpairedObjection",
                        Severity::Error,
                        format!(
                            "Method '{}' in class '{}' calls 'raise_objection' but has no corresponding 'drop_objection'. \
This un-dropped objection will cause the simulation to hang indefinitely.",
                            sub.name, cls.name
                        ),
                        r.loc,
                    );
                }
                (None, Some(k)) => {
                    out.add(
                        "UVM-OBJ-001",
                        "UvmUnpairedObjection",
                        Severity::Error,
                        format!(
                            "Method '{}' in class '{}' calls 'drop_objection' without any prior 'raise_objection' in the same scope. \
Dropping unraised objections leads to UVM runtime fatal errors.",
                            sub.name, cls.name
                        ),
                        k.loc,
                    );
                }
                _ => {}
            }
        }
    }
}
