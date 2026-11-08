//! SV-STY-005 (MandatoryMatchingEndLabel): Enforces explicit and matching end
//! labels on functions, tasks, and classes (e.g., endfunction : name).

use crate::support::source_layout::{classes, end_and_label, end_label_fix, end_loc, routine_name, routines};
use crate::syntax::Cst;
use crate::{Severity, Sink};

pub(crate) fn missing_end_label(c: &Cst, s: &mut Sink) {
    for (n, _) in routines(c) {
        let Some((end, None)) = end_and_label(c, n, &["endfunction", "endtask"]) else { continue };
        let name = routine_name(c, n);
        // An out-of-block method (cls::name) is labelled with its own name.
        let fix = end_label_fix(c, end, name.rsplit("::").next().unwrap_or(&name));
        s.add(
            "SV-STY-005",
            "MandatoryMatchingEndLabel",
            Severity::Info,
            format!(
                "Missing explicit end label on routine '{name}'. By convention, routines must terminate with a matching label ('endfunction : {name}' or 'endtask : {name}')."
            ),
            end_loc(c, end),
        )
        .fixes
        .extend(fix);
    }
    for (n, name) in classes(c) {
        let Some((end, None)) = end_and_label(c, n, &["endclass"]) else { continue };
        let fix = end_label_fix(c, end, &name);
        s.add(
            "SV-STY-005",
            "MandatoryMatchingEndLabel",
            Severity::Info,
            format!(
                "Missing explicit end label on class '{name}'. By convention, classes must terminate with a matching label ('endclass : {name}')."
            ),
            end_loc(c, end),
        )
        .fixes
        .extend(fix);
    }
}
