//! UVM-OBJ-001 (UvmUnpairedObjection): Every phase.raise_objection() call must
//! have a corresponding phase.drop_objection() within the same execution
//! context to prevent simulation hangs or fatal errors.

use crate::sem::objections::{Half, Objections};
use crate::support::uvm_classes::Ix;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::{Design, UvmKind};

pub(crate) fn uvm_unpaired_objection(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Ix::new(d, csts);
    let mut pairs: Option<Objections> = None;
    // The other half is in another routine of the class or of a base class.
    let mut elsewhere =
        |cls, routine: &str, half| pairs.get_or_insert_with(|| Objections::new(&ix)).elsewhere(&ix, cls, routine, half);
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
                (Some(_), None) if elsewhere(cls.id, &sub.name, Half::Drop) => {}
                (None, Some(_)) if elsewhere(cls.id, &sub.name, Half::Raise) => {}
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
