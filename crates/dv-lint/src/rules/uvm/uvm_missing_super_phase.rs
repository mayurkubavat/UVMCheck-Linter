//! UVM-PHS-001 (UvmMissingSuperPhase): Overridden standard UVM phases (e.g.,
//! build_phase, connect_phase) in uvm_component must call
//! `super.<phase_name>(phase)` to ensure proper base-class configuration.

use crate::support::uvm_kinds::is_component;
use crate::{is_function_phase, is_task_phase, Severity, Sink};
use dv_model::Design;

pub(crate) fn uvm_missing_super_phase(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || cls.name == "uvm_component" || !is_component(d, cls) {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || !(is_function_phase(&sub.name) || is_task_phase(&sub.name)) {
                continue;
            }
            if sub.calls.iter().any(|c| c.name == sub.name) {
                continue;
            }
            out.add(
                "UVM-PHS-001",
                "UvmMissingSuperPhase",
                Severity::Warning,
                format!("Method '{0}' in class '{1}' does not call super.{0}(phase).", sub.name, cls.name),
                sub.loc,
            );
        }
    }
}
