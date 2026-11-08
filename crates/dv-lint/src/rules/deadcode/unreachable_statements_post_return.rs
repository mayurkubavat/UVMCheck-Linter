//! SV-DED-002 (UnreachableStatementsPostReturn): Detects unreachable statements
//! appearing after an unconditional control-flow termination (return, break,
//! continue).

use crate::support::statements::{list, report, start, statement_lists, stmt, St, Units};
use crate::syntax::Cst;
use crate::{Severity, Sink};

pub(crate) fn unreachable_after_exit(c: &Cst, s: &mut Sink) {
    let units = Units::new(c);
    for b in statement_lists(c) {
        let mut term: Option<&str> = None;
        for x in list(c, b) {
            let k = stmt(c, x);
            if let Some(kind) = term {
                if k != Some(St::Empty) {
                    report(
                        s,
                        units.times(c, b),
                        "SV-DED-002",
                        "UnreachableStatementsPostReturn",
                        Severity::Warning,
                        format!(
                            "Unreachable code statement following unconditional control-flow termination ('{kind}'). \
                             Subsequent statements in this block can never execute."
                        ),
                        start(c, x),
                    );
                    break;
                }
            }
            match k {
                Some(St::Return) => term = Some("return"),
                Some(St::Break) => term = Some("break"),
                Some(St::Continue) => term = Some("continue"),
                _ => {}
            }
        }
    }
}
