//! UVM-PHS-011 (RunPhaseWithRuntimePhases): Flags a component that overrides
//! run_phase and also one of the runtime sub-phases (reset, configure, main,
//! shutdown and their pre_/post_ phases). run_phase runs in parallel with all
//! of them, so the two schedules race and readers cannot tell the order.

use dv_model::Design;
use crate::support::class_roles::{role, user_classes};
use crate::{Severity, Sink};

pub(crate) fn run_phase_with_runtime_phases(d: &Design, out: &mut Sink) {
    for cls in user_classes(d) {
        if !crate::support::class_roles::is_component_role(role(d, cls)) {
            continue;
        }
        // Subroutine members only: extern prototypes are not.
        let own = cls.methods.iter().filter(|m| !m.is_extern && !m.is_builtin);
        let mut run = None;
        let mut runtime = Vec::new();
        for m in own {
            if m.name == "run_phase" {
                run = Some(m);
            } else if m.name != "run_phase" && crate::is_task_phase(&m.name) {
                runtime.push(m.name.as_str());
            }
        }
        let Some(run) = run else { continue };
        if runtime.is_empty() {
            continue;
        }
        out.add(
            "UVM-PHS-011",
            "RunPhaseWithRuntimePhases",
            Severity::Warning,
            format!(
                "'{}' overrides run_phase and {}. run_phase runs alongside every runtime phase, so its stimulus or checks race \
the reset/configure/main schedule. Use either run_phase or the runtime phases.",
                cls.name,
                runtime.join(", ")
            ),
            run.loc,
        );
    }
}
