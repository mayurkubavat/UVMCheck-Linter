//! SV-MET-009 (ConfigKnobBudget): Flags a configuration object (a uvm_object
//! named *cfg* or *config*) that declares more than max_knobs (default 12) data
//! members of its own.

use crate::support::class_roles::{role, user_classes, Role};
use crate::{Severity, Sink};
use dv_model::Design;

pub(crate) fn config_knob_budget(d: &Design, out: &mut Sink) {
    // Option `max_knobs` (rules.SV-MET-009.options).
    fn limit() -> usize {
        crate::options::get("SV-MET-009", "max_knobs", 12) as usize
    }
    for cls in user_classes(d) {
        if role(d, cls) != Role::Config {
            continue;
        }
        let knobs = cls.properties.iter().filter(|p| !p.is_static && !p.is_const).count();
        if knobs <= limit() {
            continue;
        }
        out.add(
            "SV-MET-009",
            "ConfigKnobBudget",
            Severity::Warning,
            format!(
                "Config '{}' has {knobs} knobs (budget {}). Group related knobs into sub-config objects so each agent or feature \
reads only its own.",
                cls.name,
                limit()
            ),
            cls.loc,
        );
    }
}
