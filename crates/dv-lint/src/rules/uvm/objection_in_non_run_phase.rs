//! UVM-PHS-009 (ObjectionInNonRunPhase): Disallows calling raise_objection() or
//! drop_objection() inside non-run function phases (e.g. build_phase,
//! connect_phase).

use dv_model::{Design, PhaseKind};
use crate::{Severity, Sink};

pub(crate) fn objection_in_non_run_phase(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.phase != PhaseKind::Function {
                continue;
            }
            for c in &sub.calls {
                if c.name != "raise_objection" && c.name != "drop_objection" {
                    continue;
                }
                out.add(
                    "UVM-PHS-009",
                    "ObjectionInNonRunPhase",
                    Severity::Error,
                    format!(
                        "Objection method '.{}()' called inside function phase '{}'. \
In UVM methodology (IEEE 1800.2 § 9.2), phase objections control progression and completion strictly \
during runtime phases ('run_phase' and scheduled sub-phases). Function phases execute synchronously in \
zero simulation time; raising or dropping objections in function phases is prohibited and corrupts UVM objection counting.",
                        c.name, sub.name
                    ),
                    c.loc,
                );
            }
        }
    }
}
