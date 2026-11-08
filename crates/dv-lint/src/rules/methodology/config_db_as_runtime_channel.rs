//! UVM-CFG-007 (ConfigDbAsRuntimeChannel): Flags uvm_config_db get, set, exists
//! and wait_modified calls in the run-time phases of components and inside
//! loops of any task. config_db is build-time wiring; used at run time it
//! becomes a slow global mailbox (common coupling, MG-2.3).

use dv_model::Design;
use sv_syntax::NodeId;
use crate::support::routine_bodies::{bodies, calls_in, is_config_db_call, Calls};
use crate::support::class_roles::{is_component_role, role, Index};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

fn is_run_time_phase(m: &str) -> bool {
    crate::is_task_phase(m)
}

fn config_db_access<'a>(calls: &Calls, c: &'a Cst, call: NodeId) -> Option<&'a str> {
    ["get", "set", "exists", "wait_modified"]
        .into_iter()
        .find(|m| is_config_db_call(calls, c, call, m))
        .map(|_| c.callee_name(call))
}

pub(crate) fn config_db_as_runtime_channel(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for (cls, m, c, body) in bodies(d, &ix) {
        let calls = Calls::new(m);
        let run_time = is_component_role(role(d, cls)) && is_run_time_phase(&m.name);
        for call in calls_in(c, body) {
            let Some(name) = config_db_access(&calls, c, call) else { continue };
            let mut in_loop = false;
            let mut p = c.parent(call);
            while let Some(x) = p {
                if x == body {
                    break;
                }
                if matches!(c.kind(x), LoopStmt | DoWhileStmt) {
                    in_loop = true;
                    break;
                }
                p = c.parent(x);
            }
            let in_task_loop = in_loop && m.is_task;
            if !run_time && !in_task_loop {
                continue;
            }
            out.add(
                "UVM-CFG-007",
                "ConfigDbAsRuntimeChannel",
                Severity::Warning,
                format!(
                    "'{}::{}' calls uvm_config_db::{name}() {}. config_db is for wiring the testbench while it is built; used as a \
run-time mailbox it is slow (a regex lookup per call) and hides who talks to whom. Read the value once in build_phase, and send run-time changes through a \
TLM port, an event or a method of the config object.",
                    cls.name,
                    m.name,
                    if run_time { "during the run-time phases" } else { "inside a loop of a task" }
                ),
                c.loc_of(call),
            );
        }
    }
}
