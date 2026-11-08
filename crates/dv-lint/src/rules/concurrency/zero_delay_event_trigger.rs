//! SV-RCE-001 (ZeroDelayEventTrigger): Flags blocking event triggers ('->
//! event') which cause zero-delay simulation race conditions.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn zero_delay_event_trigger(c: &Cst, s: &mut Sink) {
    for n in c.nodes(EventTrigger) {
        let Some(trigger) = c.tok(n, "->") else { continue };
        let Some(name) = c.items(n).into_iter().next() else { continue };
        let ev = c.last_text(name);
        s.add(
            "SV-RCE-001",
            "ZeroDelayEventTrigger",
            Severity::Warning,
            format!(
                "Blocking event trigger '-> {ev}' executed in Active region. \
                 If waiting threads have not yet evaluated '@({ev})' within the current time step, the trigger is permanently missed, creating a classic zero-delay race condition. \
                 Use non-blocking event trigger '->> {ev}' to schedule the event in the NBA region, or use persistent event checking 'wait({ev}.triggered)' per IEEE 1800 § 15.5."
            ),
            c.loc(trigger),
        )
        .fixes
        .extend(crate::fix::with_safety(
            c.token_fix(trigger, trigger, "->>", "Replace blocking event trigger '->' with non-blocking event trigger '->>'"),
            crate::fix::FixSafety::Unsafe,
        ));
    }
}
