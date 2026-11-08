//! SV-STY-006 (MismatchedEndLabel): Flags mismatched end labels on functions,
//! tasks, and classes (e.g., function void foo(); ... endfunction : bar).

use crate::support::source_layout::{classes, end_and_label, routine_name, routines};
use crate::syntax::Cst;
use crate::{Severity, Sink};

pub(crate) fn mismatched_end_label(c: &Cst, s: &mut Sink) {
    for (n, _) in routines(c) {
        let Some((_, Some(label))) = end_and_label(c, n, &["endfunction", "endtask"]) else { continue };
        let name = routine_name(c, n);
        let l = c.text_of(label);
        if l == name || name.ends_with(&format!("::{l}")) {
            continue;
        }
        // The routine's own name: uvmcheck writes `cls::f` for an
        // out-of-block method, which is not a legal end label.
        let fix = c.token_fix(label, label, name.rsplit("::").next().unwrap_or(&name), "Fix mismatched end label");
        s.add(
            "SV-STY-006",
            "MismatchedEndLabel",
            Severity::Warning,
            format!(
                "Mismatched end label ':{l}' does not match routine name '{name}'. Update the label to match the routine declaration."
            ),
            c.loc(label),
        )
        .fixes
        .extend(fix);
    }
    for (n, name) in classes(c) {
        let Some((_, Some(label))) = end_and_label(c, n, &["endclass"]) else { continue };
        let l = c.text_of(label);
        if l == name {
            continue;
        }
        let fix = c.token_fix(label, label, name.as_str(), "Fix mismatched end label");
        s.add(
            "SV-STY-006",
            "MismatchedEndLabel",
            Severity::Warning,
            format!(
                "Mismatched end label ':{l}' does not match class name '{name}'. Update the label to match the class declaration."
            ),
            c.loc(label),
        )
        .fixes
        .extend(fix);
    }
}
