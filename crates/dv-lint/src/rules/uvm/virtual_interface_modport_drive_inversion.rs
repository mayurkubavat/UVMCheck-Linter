//! UVM-VIF-DIR (VirtualInterfaceModportDriveInversion): Detects procedural
//! signal driving (via '=' or '<=') on virtual interface handles inside passive
//! uvm_monitor components.

use dv_model::Design;
use crate::{derives_from, Severity, Sink};

pub(crate) fn virtual_interface_modport_drive_inversion(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        if !cls.name.contains("monitor") && !derives_from(d, Some(cls.id), "uvm_monitor") {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern {
                continue;
            }
            for a in &sub.assignments {
                let lhs = &a.lhs;
                if !lhs.contains("vif.") && !lhs.contains("intf.") && !lhs.contains("_vif.") {
                    continue;
                }
                out.add(
                    "UVM-VIF-DIR",
                    "VirtualInterfaceModportDriveInversion",
                    Severity::Error,
                    format!(
                        "Passive uvm_monitor '{}' illegally drives virtual interface signal '{lhs}'. Monitors must be strictly read-only observers. Driving signals from a monitor \
violates UVM passive component semantics and causes multi-driver contention.",
                        cls.name
                    ),
                    a.loc,
                );
            }
        }
    }
}
