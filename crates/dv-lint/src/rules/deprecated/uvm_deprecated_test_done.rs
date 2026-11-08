//! UVM-DEP-004 (UvmDeprecatedTestDone): Disallow obsolete OVM 'uvm_test_done'
//! objection handle (IEEE 1800.2 Annex H).

use crate::support::copies::{copies, routine_calls};
use crate::syntax::Cst;
use crate::{Severity, Sink};

pub(crate) fn deprecated_test_done(c: &Cst, s: &mut Sink) {
    for (n, name, first) in routine_calls(c, &|x| x == "raise_objection" || x == "drop_objection") {
        if !c.text(n).contains("uvm_test_done") {
            continue;
        }
        for _ in 0..copies(c, n) {
            s.add(
                "UVM-DEP-004",
                "UvmDeprecatedTestDone",
                Severity::Error,
                format!(
                    "Legacy OVM objection handle 'uvm_test_done' is obsolete in IEEE 1800.2 Annex H. \
                     Objections must be attached directly to the active phase handle ('phase.{name}(this)')."
                ),
                c.loc(first),
            );
        }
    }
}
