//! SV-MET-003 (TooManyArguments): Flags tasks and functions with more formal
//! arguments than the threshold (default 7; Pylint R0913, Sonar S107).

use crate::support::statements::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::routine_metrics::{each_routine};

pub(crate) fn too_many_arguments(c: &Cst, s: &mut Sink) {
    let limit = 7;
    each_routine(c, |classes, r| {
        if r.name == "new" && r.class.is_some() {
            return;
        }
        if let Some(i) = r.class {
            if classes.override_of(i, &r.name).is_some() {
                return;
            }
        }
        if r.ports <= limit {
            return;
        }
        report(
            s,
            r.times,
            "SV-MET-003",
            "TooManyArguments",
            Severity::Warning,
            format!(
                "'{}' takes {} arguments (limit {limit}). Pass the transaction or a config object instead, so adding \
                 a protocol field does not change every caller.",
                r.name, r.ports
            ),
            c.loc(r.tok),
        );
    });
}
