//! UVM-SEQ-012 (SequenceStartOnNullSequencer): Flags sequence start(null) calls
//! that start sequences without an attached sequencer handle.

use crate::support::cross_labels::last_line;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::Child;
use sv_syntax::SyntaxKind::*;

pub(crate) fn start_on_null(c: &Cst, s: &mut Sink) {
    for call in c.nodes(CallExpr) {
        let Some(f) = c.tree.children(call).next() else { continue };
        if c.last_text(f) != "start" {
            continue;
        }
        let Some(list) = c.tree.child(call, ArgList) else { continue };
        // The first argument: the node before any comma.
        let first =
            c.kids(list).iter().take_while(|k| !matches!(**k, Child::Token(t) if c.text_of(t) == ",")).find_map(|k| {
                match *k {
                    Child::Node(a) => Some(a),
                    Child::Token(_) => None,
                }
            });
        let Some(arg) = first.filter(|&a| c.kind(a) == Arg && c.tok(a, ".").is_none()) else { continue };
        let Some(e) = c.tree.children(arg).next() else { continue };
        if c.trimmed(e) != "null" {
            continue;
        }
        let callee = c.text(f);
        let seq = match callee.rfind(".start") {
            Some(p) => last_line(&callee[..p]).to_string(),
            None => "seq".to_string(),
        };
        let Some(t) = c.first_tok(e) else { continue };
        s.add(
            "UVM-SEQ-012",
            "SequenceStartOnNullSequencer",
            Severity::Warning,
            format!(
                "Sequence invocation '{seq}.start(null)' starts sequence with null sequencer handle. \
                 In UVM sequence architecture (IEEE 1800.2 § 5.8), \
                 starting a sequence on 'null' leaves 'm_sequencer' and 'p_sequencer' unattached, causing immediate \
                 fatal null-pointer crashes when the sequence attempts to send transactions (start_item/finish_item) \
                 or query sequencer configuration. Pass a valid sequencer reference (e.g. 'm_sequencer' or 'p_sequencer')."
            ),
            c.loc(t),
        );
    }
}
