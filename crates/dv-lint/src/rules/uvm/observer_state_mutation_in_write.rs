//! UVM-OBS-001 (ObserverStateMutationInWrite): Mutating the observed
//! transaction object inside a subscriber's write() method corrupts state for
//! other concurrent subscribers in the Observer pattern.

use dv_model::{Design, RootKind};
use crate::{Severity, Sink};

pub(crate) fn observer_state_mutation_in_write(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.name != "write" || sub.is_task {
                continue;
            }
            let Some(a0) = sub.arguments.first() else {
                continue;
            };
            if a0.type_class.is_none() {
                continue;
            }
            let arg = &a0.name;
            for a in &sub.assignments {
                if a.root_kind != RootKind::Argument || &a.root_name != arg {
                    continue;
                }
                out.add(
                    "UVM-OBS-001",
                    "ObserverStateMutationInWrite",
                    Severity::Error,
                    format!(
                        "In-place mutation of observed transaction argument '{arg}' in 'write()' method of class '{}'. \
In the UVM Observer pattern, the same transaction handle is broadcast to multiple subscribers \
(scoreboards, functional coverage, checkers). Mutating transaction fields directly corrupts data \
for all downstream observers. Clone the transaction ($cast(cloned_t, {arg}.clone())) before mutating.",
                        cls.name
                    ),
                    a.loc,
                );
            }
        }
    }
}
