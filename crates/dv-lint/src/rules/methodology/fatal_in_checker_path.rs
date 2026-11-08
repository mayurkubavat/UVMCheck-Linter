//! UVM-REP-008 (FatalInCheckerPath): Flags `uvm_fatal on a data mismatch in
//! scoreboards and subscribers.

use dv_model::Design;
use crate::support::routine_bodies::bodies;
use crate::support::class_roles::{role, role_name, Index, Role};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::check_failures::{Level, failure_branches, strongest_report};

pub(crate) fn fatal_in_checker_path(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for (cls, m, c, body) in bodies(d, &ix) {
        let r = role(d, cls);
        if r != Role::Scoreboard && r != Role::Subscriber {
            continue;
        }
        for (b, cond, what) in failure_branches(c, body) {
            if strongest_report(c, b) != Level::Fatal {
                continue;
            }
            out.add(
                "UVM-REP-008",
                "FatalInCheckerPath",
                Severity::Warning,
                format!(
                    "{} '{}' stops the run with `uvm_fatal when a check ({what}) fails in {}(). One mismatch then hides all later ones and \
skips the report phase summary; report it with `uvm_error.",
                    role_name(r),
                    cls.name,
                    m.name
                ),
                c.loc_of(cond),
            );
        }
    }
}
