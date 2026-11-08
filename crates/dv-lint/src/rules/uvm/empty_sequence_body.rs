//! UVM-SEQ-005 (EmptySequenceBody): Detects sequence classes whose body() task
//! contains no executable statements.

use crate::support::uvm_classes::{sequence_bodies, Ix};
use crate::support::uvm_syntax::body_items;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use sv_syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn empty_sequence_body(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for c in csts {
        for (cls, f) in sequence_bodies(&ix, c) {
            if body_items(c, f).iter().any(|&i| c.kind(i) != EmptyStmt) {
                continue;
            }
            let Some(at) = crate::support::uvm_syntax::name_loc(c, f) else { continue };
            s.add(
                "UVM-SEQ-005",
                "EmptySequenceBody",
                Severity::Warning,
                format!(
                    "Sequence class '{cls}' contains an empty 'body()' task. Empty sequences produce no stimulus transactions and typically indicate incomplete VIP or obsolete test scaffolding."
                ),
                at,
            );
        }
    }
}
