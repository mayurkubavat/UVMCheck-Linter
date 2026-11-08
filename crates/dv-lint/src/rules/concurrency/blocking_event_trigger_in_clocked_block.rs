//! SV-CON-013 (BlockingEventTriggerInClockedBlock): Disallows blocking event
//! triggers (-> ev) inside clocked blocks; non-blocking triggers (->> ev)
//! should be used instead.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn blocking_trigger(c: &Cst, s: &mut Sink) {
    for n in c.nodes(EventTrigger) {
        let Some(arrow) = c.tok(n, "->") else { continue };
        let mut clocked = false;
        let mut up = c.parent(n);
        while let Some(p) = up {
            let kw = c.keyword(p).map(|k| c.text_of(k));
            if c.kind(p) == ProceduralBlock && kw == Some("always_ff") {
                clocked = true;
                break;
            }
            if c.kind(p) == TimedStmt || (c.kind(p) == ProceduralBlock && kw == Some("always")) {
                let text = c.text(p);
                if text.contains("posedge") || text.contains("negedge") {
                    clocked = true;
                    break;
                }
            }
            up = c.parent(p);
        }
        if !clocked {
            continue;
        }
        let ev = c.tree.children(n).next().map(|e| c.text(e)).unwrap_or_else(|| "event".to_string());
        s.add(
            "SV-CON-013",
            "BlockingEventTriggerInClockedBlock",
            Severity::Warning,
            format!(
                "Blocking event trigger '-> {ev}' is invoked within a sequential clocked process. In SystemVerilog (IEEE 1800 § 15.5 and \
                 IEEE 1800 § 15.5), clocked processes should trigger events using \
                 non-blocking trigger ('->> {ev}') to schedule notification in the Nonblocking Events \
                 region, eliminating zero-time race conditions with concurrent listener threads."
            ),
            c.loc(arrow),
        );
    }
}
