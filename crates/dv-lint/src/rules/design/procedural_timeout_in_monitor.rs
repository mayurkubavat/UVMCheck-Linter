//! UVM-ROL-007 (ProceduralTimeoutInMonitor): Flags fork ... join_any blocks in
//! a monitor where one branch counts clock cycles (repeat (N) @(...)) and then
//! reports an error: a handshake timeout written as procedural code.

use crate::support::class_index::{all_stmts, strongest_report, Classes, Level, Role};
use crate::support::statements::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

pub(crate) fn procedural_timeout(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    for i in classes.user() {
        if classes.role(i) != Role::Monitor {
            continue;
        }
        let cls = &classes.list[i];
        for m in cls.methods.iter().filter(|m| !m.out_of_block) {
            let Some(body) = m.body else { continue };
            for b in all_stmts(c, body) {
                if stmt(c, b) != Some(St::Par(Join::Any)) || !has_counting_branch(c, b) {
                    continue;
                }
                s.add(
                    "UVM-ROL-007",
                    "ProceduralTimeoutInMonitor",
                    Severity::Info,
                    format!(
                        "Monitor '{}' times a handshake by counting cycles in a \
                         fork ... join_any. Write the rule as an assertion in the interface (req |-> \
                         ##[1:N] ack) and keep the monitor to collecting transactions.",
                        cls.name
                    ),
                    start(c, b),
                );
            }
        }
    }
}

fn has_counting_branch(c: &Cst, body: NodeId) -> bool {
    let stmts = all_stmts(c, body);
    let repeat = stmts.iter().any(|&r| {
        stmt(c, r) == Some(St::Repeat)
            && body_of(c, r).is_some_and(|b| {
                all_stmts(c, b).into_iter().any(|t| {
                    stmt(c, t) == Some(St::Timed)
                        && c.tree
                            .child(t, TimingControl)
                            .and_then(|tc| c.keyword(tc))
                            .is_some_and(|k| c.text_of(k) == "@")
                })
            })
    });
    let level = strongest_report(c, body);
    repeat && (level == Level::Error || level == Level::Fatal)
}
