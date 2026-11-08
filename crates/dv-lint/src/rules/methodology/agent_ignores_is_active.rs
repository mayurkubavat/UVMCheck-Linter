//! UVM-RUS-001 (AgentIgnoresIsActive): Flags agents that create their driver or
//! sequencer without checking is_active or get_is_active().

use dv_model::Design;
use crate::support::class_roles::{role, role_name, user_classes, Index, Role};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::routine_bodies::{class_creations};

pub(crate) fn agent_ignores_is_active(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for cls in user_classes(d) {
        if role(d, cls) != Role::Agent {
            continue;
        }
        for cr in class_creations(d, &ix, cls) {
            let r = role(d, cr.class);
            if cr.guarded || (r != Role::Driver && r != Role::Sequencer) {
                continue;
            }
            out.add(
                "UVM-RUS-001",
                "AgentIgnoresIsActive",
                Severity::Warning,
                format!(
                    "Agent '{}' creates {} '{}' without checking get_is_active(). In a passive agent the driver would still fight the DUT for the bus. Create the \
driver and sequencer only inside 'if (get_is_active() == UVM_ACTIVE)'.",
                    cls.name,
                    role_name(r),
                    cr.class.name
                ),
                cr.loc,
            );
        }
    }
}
