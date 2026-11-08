//! UVM-ROL-006 (VirtualInterfaceInChecker): Flags virtual interface handles in
//! scoreboards and subscribers.

use dv_model::Design;
use crate::support::class_roles::{role, role_name, user_classes, Role};
use crate::{Severity, Sink};

pub(crate) fn virtual_interface_in_checker(d: &Design, out: &mut Sink) {
    for cls in user_classes(d) {
        let r = role(d, cls);
        if r != Role::Scoreboard && r != Role::Subscriber {
            continue;
        }
        for p in cls.properties.iter().filter(|p| p.is_virtual_interface) {
            out.add(
                "UVM-ROL-006",
                "VirtualInterfaceInChecker",
                Severity::Warning,
                format!(
                    "{} '{}' holds virtual interface '{}'. Have a monitor publish what the pins show (reset included) as \
transactions, and check those.",
                    role_name(r),
                    cls.name,
                    p.name
                ),
                p.loc,
            );
        }
    }
}
