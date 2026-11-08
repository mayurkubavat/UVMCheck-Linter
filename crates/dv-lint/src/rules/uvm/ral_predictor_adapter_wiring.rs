//! RAL-WIRE-001 (RalPredictorAdapterWiring): Verifies that all instantiated
//! uvm_reg_predictor components have their .map property assigned in
//! connect_phase.

use dv_model::Design;
use crate::{Severity, Sink};
use crate::support::connect_phase::{connect_phase_assignments};

pub(crate) fn ral_predictor_adapter_wiring(d: &Design, out: &mut Sink) {
    let mut predictors = Vec::new();
    for cls in d.classes() {
        if cls.is_parameterized || !cls.uvm.is_component() {
            continue;
        }
        for p in &cls.properties {
            if p.ty.contains("reg_predictor") {
                predictors.push((cls, p));
            }
        }
    }
    if predictors.is_empty() {
        return;
    }
    let assignments = connect_phase_assignments(d);
    for (owner, p) in predictors {
        let target = format!("{}.map", p.name);
        if assignments.iter().any(|s| s.contains(&target)) {
            continue;
        }
        out.add(
            "RAL-WIRE-001",
            "RalPredictorAdapterWiring",
            Severity::Warning,
            format!(
                "Register predictor '{0}' in component '{1}' is missing mandatory '.map' assignment in connect_phase (e.g. '{0}.map = reg_model.default_map;'). \
Without map assignment, register prediction will fail silently and mirror values will not update.",
                p.name, owner.name
            ),
            p.loc,
        );
    }
}
