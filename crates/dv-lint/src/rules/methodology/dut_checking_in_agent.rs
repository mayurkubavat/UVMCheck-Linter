//! UVM-ROL-003 (DutCheckingInAgent): Flags agents that hold a scoreboard,
//! predictor or reference model.

use dv_model::Design;
use crate::support::class_roles::{is_library, property_class, role, user_classes, Role};
use crate::{Severity, Sink};

fn is_checker_class(d: &Design, c: &dv_model::Class) -> bool {
    if is_library(d, c) {
        return false;
    }
    if role(d, c) == Role::Scoreboard {
        return true;
    }
    let n = c.name.to_ascii_lowercase();
    ["scoreboard", "predictor", "ref_model", "refmodel", "reference_model"]
        .iter()
        .any(|w| n.contains(w))
}

pub(crate) fn dut_checking_in_agent(d: &Design, out: &mut Sink) {
    for cls in user_classes(d) {
        if role(d, cls) != Role::Agent {
            continue;
        }
        for p in &cls.properties {
            let Some(t) = property_class(d, cls, p) else { continue };
            if !is_checker_class(d, t) {
                continue;
            }
            out.add(
                "UVM-ROL-003",
                "DutCheckingInAgent",
                Severity::Warning,
                format!(
                    "Agent '{}' contains '{}' of type '{}'. Expected-value models and scoreboards are specific to one DUT, so the agent cannot be reused for another \
one. Build them in the env and connect them to the agent's analysis port.",
                    cls.name, p.name, t.name
                ),
                p.loc,
            );
        }
    }
}
