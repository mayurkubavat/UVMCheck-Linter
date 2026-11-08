//! UVM-SEQ-011 (UncheckedPSequencerCast): Flags unchecked $cast(p_sequencer,
//! m_sequencer) calls in sequences that do not verify the return status.

use crate::support::uvm_classes::args;
use crate::support::uvm_syntax::{callee, stmt_expr};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use sv_syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn unchecked_p_sequencer_cast(_d: &Design, csts: &[Cst], s: &mut Sink) {
    for c in csts {
        for st in c.nodes(ExprStmt) {
            let Some(mut e) = stmt_expr(c, st) else { continue };
            if c.kind(e) == CastExpr {
                let items = c.items(e);
                if items.len() != 2 || c.tree.bare_text(items[0]) != "void" {
                    continue;
                }
                e = items[1];
            }
            if c.kind(e) != CallExpr || callee(c, e) != "$cast" {
                continue;
            }
            let a = args(c, e);
            if a.len() < 2 {
                continue;
            }
            let (Some(_), Some(src)) = (a[0], a[1]) else { continue };
            let src = c.text(src);
            let call = c.text(e);
            if !(src.contains("m_sequencer") || call.contains("p_sequencer") || call.contains("m_sequencer")) {
                continue;
            }
            let src = if src.is_empty() { "m_sequencer".to_string() } else { src };
            let Some(t) = c.first_tok(st) else { continue };
            s.add(
                "UVM-SEQ-011",
                "UncheckedPSequencerCast",
                Severity::Warning,
                format!(
                    "Dynamic cast '$cast(p_sequencer, {src})' is executed without checking its return status. \
                     In UVM sequence architecture (IEEE 1800.2 § 5.8), if the sequence is started on an \
                     incompatible sequencer or virtual sequencer, an unchecked $cast leaves 'p_sequencer' null, resulting in an immediate \
                     null-pointer simulation crash. Always check the return value (e.g. 'if (!$cast(p_sequencer, m_sequencer)) `uvm_fatal(...)')."
                ),
                c.loc(t),
            );
        }
    }
}
