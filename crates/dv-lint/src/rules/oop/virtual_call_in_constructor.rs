//! SV-OOP-005 (VirtualCallInConstructor): Never call virtual methods inside
//! constructors.

use crate::{Severity, Sink};
use dv_model::Design;

pub(crate) fn virtual_call_in_constructor(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.name != "new" || sub.is_builtin {
                continue;
            }
            for c in &sub.calls {
                if c.is_system || !c.callee_declared_virtual {
                    continue;
                }
                out.add(
                    "SV-OOP-005",
                    "VirtualCallInConstructor",
                    Severity::Warning,
                    format!(
                        "Virtual method '{}' is invoked inside constructor 'new' of class '{}'. In SystemVerilog, base class constructors execute before derived class members are initialized. \
Calling virtual methods in constructors executes overridden child methods against uninitialized state.",
                        c.name, cls.name
                    ),
                    c.loc,
                );
            }
        }
    }
}
