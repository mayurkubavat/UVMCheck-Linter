//! UVM-PHS-004 (SequenceInFunctionPhase): Disallow starting sequences
//! (.start()) inside zero-time function phases (build_phase, connect_phase,
//! etc.).

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

const FUNCTION_PHASES: &[&str] = &[
    "build_phase",
    "connect_phase",
    "end_of_elaboration_phase",
    "start_of_simulation_phase",
    "extract_phase",
    "check_phase",
    "report_phase",
    "final_phase",
];

pub(crate) fn sequence_in_function_phase(c: &Cst, s: &mut Sink) {
    for n in c.nodes(FunctionDecl) {
        let Some(name) = c.tree.child(n, Name).and_then(|nm| c.first_tok(nm)) else { continue };
        let f = c.text_of(name);
        if !FUNCTION_PHASES.contains(&f) {
            continue;
        }
        let body = c.text(n);
        if !body.contains(".start(") && !body.contains(".start (") {
            continue;
        }
        s.add(
            "UVM-PHS-004",
            "SequenceInFunctionPhase",
            Severity::Error,
            format!(
                "Calling sequence method '.start()' inside non-time-consuming function phase '{f}' is illegal in UVM (IEEE 1800.2 § 9.3). Function phases execute in zero simulation time, \
                 whereas sequence execution requires time-consuming clocking events and driver handshakes. \
                 Sequences must only be started in task phases such as 'run_phase' or 'main_phase'."
            ),
            c.loc(name),
        );
    }
}
