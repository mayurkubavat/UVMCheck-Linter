//! UVM-PHS-002 (UvmTimeInFunctionPhase): Function-based UVM phases (e.g.,
//! build_phase, connect_phase) must not consume simulation time using delays
//! (#), event controls (@), or wait statements.

use crate::{Severity, Sink};
use dv_model::{Design, PhaseKind};

pub(crate) fn uvm_time_in_function_phase(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || !cls.uvm.is_component() {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.phase != PhaseKind::Function {
                continue;
            }
            // The last one, as the C++ rule reports.
            let Some(&last) = sub.timing_controls.last() else {
                continue;
            };
            out.add(
                "UVM-PHS-002",
                "UvmTimeInFunctionPhase",
                Severity::Error,
                format!(
                    "Time-consuming statement detected inside function phase '{}' in class '{}'.",
                    sub.name, cls.name
                ),
                last,
            );
        }
    }
}
