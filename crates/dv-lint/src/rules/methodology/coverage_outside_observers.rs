//! UVM-ROL-002 (CoverageOutsideObservers): Flags covergroups declared in
//! drivers and sequences.

use crate::support::class_roles::{role, role_name, user_classes, Role};
use crate::{Severity, Sink};
use dv_model::Design;

pub(crate) fn coverage_outside_observers(d: &Design, out: &mut Sink) {
    for cls in user_classes(d) {
        let r = role(d, cls);
        if r != Role::Driver && r != Role::Sequence {
            continue;
        }
        for p in &cls.properties {
            if !p.ty.contains("covergroup") {
                continue;
            }
            out.add(
                "UVM-ROL-002",
                "CoverageOutsideObservers",
                Severity::Warning,
                format!(
                    "{} '{}' holds covergroup '{}'. Coverage sampled here records stimulus that was requested, even if the DUT never saw it, and is \
lost when the agent runs passive. Sample it in a monitor or a coverage subscriber fed from the monitor's analysis port.",
                    role_name(r),
                    cls.name,
                    p.name
                ),
                p.loc,
            );
        }
    }
}
