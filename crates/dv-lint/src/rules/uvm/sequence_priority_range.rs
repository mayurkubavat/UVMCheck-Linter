//! UVM-SEQ-007 (SequencePriorityRange): Flags sequence start() calls passing
//! invalid negative priority values (< -1) which corrupt sequencer arbitration.

use sv_syntax::Child;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn sequence_priority(c: &Cst, s: &mut Sink) {
    for call in c.nodes(CallExpr) {
        let Some(callee) = c.tree.children(call).next() else { continue };
        if c.last_text(callee) != "start" {
            continue;
        }
        let Some(args) = c.tree.child(call, ArgList) else { continue };
        // The third argument: the node between the second and third commas.
        let mut commas = 0;
        let mut third = None;
        for k in c.kids(args) {
            match *k {
                Child::Token(t) if c.text_of(t) == "," => commas += 1,
                Child::Node(a) if commas == 2 => third = Some(a),
                _ => {}
            }
        }
        let Some(arg) = third else { continue };
        if c.kind(arg) != Arg || c.tok(arg, ".").is_some() {
            continue;
        }
        let Some(expr) = c.tree.children(arg).next() else { continue };
        let prio = c.trimmed(expr);
        if !prio.starts_with('-') || prio == "-1" {
            continue;
        }
        let Some(t) = c.first_tok(expr) else { continue };
        s.add(
            "UVM-SEQ-007",
            "SequencePriorityRange",
            Severity::Warning,
            format!(
                "Invalid sequence priority value '{prio}' passed to 'seq.start()'. In UVM, sequence priorities must be non-negative integers >= 0 (or -1 to inherit the sequencer default priority 100). Arbitrary negative values corrupt arbitration hierarchies."
            ),
            c.loc(t),
        );
    }
}
