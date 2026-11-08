//! SV-DEP-004 (RandomSystemFunction): Disallow legacy Verilog '$random'.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::copies::{calls, copies};

pub(crate) fn random_system_function(c: &Cst, s: &mut Sink) {
    for (n, name, first) in calls(c) {
        if name != "$random" {
            continue;
        }
        for _ in 0..copies(c, n) {
            s.add(
                "SV-DEP-004",
                "RandomSystemFunction",
                Severity::Warning,
                "Verilog system function '$random' is superseded in IEEE 1800 § 20.15. \
                 '$random' generates signed 32-bit integers that evaluate to negative values when used with modulo, \
                 and shares a single global LCG seed that corrupts simulation random stability. \
                 Replace with '$urandom' or '$urandom_range(min, max)'."
                    .into(),
                c.loc(first),
            );
        }
    }
}
