//! Assignments in connect_phase.

use dv_model::Design;

/// Every `lhs = rhs` in any class's connect_phase, parameterized ones included.
pub(crate) fn connect_phase_assignments(d: &Design) -> Vec<String> {
    d.classes()
        .iter()
        .flat_map(|c| c.methods.iter())
        .filter(|m| m.name == "connect_phase")
        .flat_map(|m| m.assignments.iter())
        .map(|a| format!("{} = {}", a.lhs, a.rhs))
        .collect()
}
