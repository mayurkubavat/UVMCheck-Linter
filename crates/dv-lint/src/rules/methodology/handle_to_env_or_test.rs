//! UVM-CPL-002 (HandleToEnvOrTest): Flags sequences, items, agents, config
//! objects and helpers that store a handle to a user env or test class.

use dv_model::Design;
use crate::support::class_roles::{is_library, property_class, role, role_name, user_classes, Role};
use crate::{Severity, Sink};

pub(crate) fn handle_to_env_or_test(d: &Design, out: &mut Sink) {
    for cls in user_classes(d) {
        let r0 = role(d, cls);
        if r0 == Role::Env || r0 == Role::Test {
            continue;
        }
        for p in &cls.properties {
            let Some(t) = property_class(d, cls, p) else { continue };
            if is_library(d, t) {
                continue;
            }
            let r = role(d, t);
            if r != Role::Env && r != Role::Test {
                continue;
            }
            out.add(
                "UVM-CPL-002",
                "HandleToEnvOrTest",
                Severity::Warning,
                format!(
                    "{} '{}' stores '{}', a handle to {} '{}'. Everything below the env now depends on the env's structure. Pass in only what is used: a config object, a \
sequencer handle or a TLM port.",
                    role_name(r0),
                    cls.name,
                    p.name,
                    role_name(r),
                    t.name
                ),
                p.loc,
            );
        }
    }
}
