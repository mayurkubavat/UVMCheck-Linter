//! UVM-REP-007 (CheckFailureAsWarning): Flags a failed compare() or an
//! expected-versus-actual mismatch reported only with `uvm_warning or
//! `uvm_info.

use dv_model::Design;
use crate::support::routine_bodies::bodies;
use crate::support::class_roles::Index;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::check_failures::{Level, failure_branches, strongest_report};

pub(crate) fn check_failure_as_warning(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for (cls, m, c, body) in bodies(d, &ix) {
        for (b, cond, what) in failure_branches(c, body) {
            let lvl = strongest_report(c, b);
            if lvl != Level::Warning && lvl != Level::Info {
                continue;
            }
            out.add(
                "UVM-REP-007",
                "CheckFailureAsWarning",
                Severity::Warning,
                format!(
                    "A failed check ({what}) in '{}::{}' is reported with `uvm_{}. Warnings do not fail the test, so the mismatch goes unnoticed in \
regression; report it with `uvm_error.",
                    cls.name,
                    m.name,
                    if lvl == Level::Warning { "warning" } else { "info" }
                ),
                c.loc_of(cond),
            );
        }
    }
}
