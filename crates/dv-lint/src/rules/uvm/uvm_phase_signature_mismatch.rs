//! UVM-PHS-003 (UvmPhaseSignatureMismatch): Standard UVM phase methods must
//! declare correct signatures (function void vs task, and a single argument of
//! type uvm_phase).

use dv_model::Design;
use crate::{derives_from, is_function_phase, is_task_phase, Severity, Sink};

pub(crate) fn uvm_phase_signature_mismatch(d: &Design, out: &mut Sink) {
    const ID: &str = "UVM-PHS-003";
    const NAME: &str = "UvmPhaseSignatureMismatch";
    for cls in d.classes() {
        if cls.is_parameterized || !cls.uvm.is_component() {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.is_builtin {
                continue;
            }
            let n = &sub.name;
            let (fp, tp) = (is_function_phase(n), is_task_phase(n));
            if !fp && !tp {
                continue;
            }
            let at = sub.loc;
            if fp {
                if sub.is_task {
                    out.add(
                        ID,
                        NAME,
                        Severity::Error,
                        format!(
                            "UVM phase '{n}' in class '{}' is declared as a 'task'. Standard UVM function phases must be declared as 'function void'.",
                            cls.name
                        ),
                        at,
                    );
                } else if !sub.returns_void {
                    out.add(
                        ID,
                        NAME,
                        Severity::Error,
                        format!(
                            "UVM phase '{n}' in class '{}' has return type '{}'. Standard UVM phases must return 'void'.",
                            cls.name, sub.return_type
                        ),
                        at,
                    );
                }
            }
            if tp && !sub.is_task {
                out.add(
                    ID,
                    NAME,
                    Severity::Error,
                    format!(
                        "UVM phase '{n}' in class '{}' is declared as a 'function'. Time-consuming UVM phases must be declared as 'task'.",
                        cls.name
                    ),
                    at,
                );
            }
            if sub.arguments.len() != 1 {
                out.add(
                    ID,
                    NAME,
                    Severity::Error,
                    format!(
                        "UVM phase '{n}' in class '{}' declares {} argument(s), but standard UVM phases must declare exactly one argument of type 'uvm_phase'.",
                        cls.name,
                        sub.arguments.len()
                    ),
                    at,
                );
            } else {
                let a = &sub.arguments[0];
                let ok = (a.type_class.is_some() && derives_from(d, a.type_class, "uvm_phase"))
                    || a.ty.contains("uvm_phase");
                if !ok {
                    out.add(
                        ID,
                        NAME,
                        Severity::Error,
                        format!(
                            "Argument '{}' of UVM phase '{n}' in class '{}' has type '{}', but must be of type 'uvm_phase'.",
                            a.name, cls.name, a.ty
                        ),
                        a.loc,
                    );
                }
            }
        }
    }
}
