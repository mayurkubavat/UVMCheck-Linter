//! UVM-ROL-005 (MonitorDependsOnActivePath): Flags monitors that hold a handle
//! to a driver, sequencer or sequence item port.

use dv_model::Design;
use crate::support::class_roles::{property_class, role, user_classes, Role};
use crate::{Severity, Sink};

pub(crate) fn monitor_depends_on_active_path(d: &Design, out: &mut Sink) {
    for cls in user_classes(d) {
        if role(d, cls) != Role::Monitor {
            continue;
        }
        for p in &cls.properties {
            let Some(t) = property_class(d, cls, p) else { continue };
            let r = role(d, t);
            let item_port = t.name.starts_with("uvm_seq_item_pull");
            if r != Role::Driver && r != Role::Sequencer && !item_port {
                continue;
            }
            out.add(
                "UVM-ROL-005",
                "MonitorDependsOnActivePath",
                Severity::Warning,
                format!(
                    "Monitor '{}' holds '{}' of type '{}'. Passive agents have no driver or sequencer, so the monitor fails there; observe the interface only \
and publish what you see on an analysis port.",
                    cls.name, p.name, t.name
                ),
                p.loc,
            );
        }
    }
}
