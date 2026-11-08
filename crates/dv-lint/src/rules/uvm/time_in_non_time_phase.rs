//! UVM-PHS-008 (TimeInNonTimePhase): Disallows time-consuming statements (#, @,
//! wait, ##) inside non-time-consuming UVM phases (e.g. build_phase,
//! connect_phase, check_phase).

use crate::support::uvm_syntax::{body_items, name_node, routine_last_name, routines, subtree};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

/// slang's TimingControlStatement, WaitStatement or WaitForkStatement.
fn timing_kind(c: &Cst, n: NodeId) -> Option<&'static str> {
    match c.kind(n) {
        TimedStmt => Some("timing control (#, @, or ##)"),
        WaitStmt => {
            let mut t = c.toks(n);
            match (t.next().map(|x| c.text_of(x)), t.next().map(|x| c.text_of(x))) {
                (Some("wait"), Some("fork")) => Some("wait fork"),
                (Some("wait"), _) => Some("wait (wait condition)"),
                _ => None,
            }
        }
        _ => None,
    }
}

pub(crate) fn time_in_non_time_phase(c: &Cst, s: &mut Sink) {
    const NON_TIME: [&str; 8] = [
        "build_phase",
        "connect_phase",
        "end_of_elaboration_phase",
        "start_of_simulation_phase",
        "extract_phase",
        "check_phase",
        "report_phase",
        "final_phase",
    ];
    for f in routines(c) {
        if name_node(c, f).is_none() {
            continue;
        }
        let name = routine_last_name(c, f);
        if !NON_TIME.contains(&name.as_str()) {
            continue;
        }
        let found = body_items(c, f)
            .into_iter()
            .find_map(|i| subtree(c, i).into_iter().find_map(|n| timing_kind(c, n).map(|k| (n, k))));
        let Some((n, what)) = found else { continue };
        let Some(at) = c.first_tok(n) else { continue };
        s.add(
            "UVM-PHS-008",
            "TimeInNonTimePhase",
            Severity::Error,
            format!(
                "Non-time-consuming UVM phase '{name}' contains time-consuming construct '{what}'. Non-run UVM phases execute as instantaneous functions in 0 simulation time. Timing controls inside these phases corrupt the UVM execution schedule."
            ),
            c.loc(at),
        );
    }
}
