//! SV-STY-005 (MandatoryMatchingEndLabel): Enforces explicit and matching end
//! labels on functions, tasks, and classes (e.g., endfunction : name).

use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::source_layout::{classes, end_and_label, end_loc, routine_name, routines};

pub(crate) fn missing_end_label(c: &Cst, s: &mut Sink) {
    for (n, _) in routines(c) {
        let Some((end, None)) = end_and_label(c, n, &["endfunction", "endtask"]) else { continue };
        let name = routine_name(c, n);
        s.add(
            "SV-STY-005",
            "MandatoryMatchingEndLabel",
            Severity::Info,
            format!(
                "Missing explicit end label on routine '{name}'. By convention, routines must terminate with a matching label ('endfunction : {name}' or 'endtask : {name}')."
            ),
            end_loc(c, end),
        );
    }
    for (n, name) in classes(c) {
        let Some((end, None)) = end_and_label(c, n, &["endclass"]) else { continue };
        s.add(
            "SV-STY-005",
            "MandatoryMatchingEndLabel",
            Severity::Info,
            format!(
                "Missing explicit end label on class '{name}'. By convention, classes must terminate with a matching label ('endclass : {name}')."
            ),
            end_loc(c, end),
        );
    }
}
