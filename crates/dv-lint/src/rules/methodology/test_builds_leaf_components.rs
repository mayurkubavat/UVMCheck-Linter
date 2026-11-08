//! UVM-ROL-004 (TestBuildsLeafComponents): Flags tests that create drivers,
//! monitors, sequencers, agents, scoreboards or subscribers directly.

use crate::support::class_roles::{role, role_name, user_classes, Index, Role};
use crate::support::routine_bodies::class_creations;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;

pub(crate) fn test_builds_leaf_components(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for cls in user_classes(d) {
        if role(d, cls) != Role::Test {
            continue;
        }
        for cr in class_creations(d, &ix, cls) {
            let r = role(d, cr.class);
            if !matches!(
                r,
                Role::Driver | Role::Monitor | Role::Sequencer | Role::Agent | Role::Scoreboard | Role::Subscriber
            ) {
                continue;
            }
            out.add(
                "UVM-ROL-004",
                "TestBuildsLeafComponents",
                Severity::Warning,
                format!(
                    "Test '{}' creates {} '{}' itself. Each test that builds its own components drifts from the others; create an env that builds them, and \
change it from the test through its config object or factory overrides.",
                    cls.name,
                    role_name(r),
                    cr.class.name
                ),
                cr.loc,
            );
        }
    }
}
