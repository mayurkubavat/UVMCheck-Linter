//! UVM-OBJ-004 (OrphanSubsequenceObjectionHang): Detects background processes
//! launched via fork..join_none inside UVM sequences that raise objections
//! without a deterministic drop path, leaving orphan objections that hang
//! simulation phases.

use dv_model::Design;
use crate::support::uvm_syntax::subtree;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::uvm_classes::{Ix, block_stmts, call_name, calls_in, is_join_none, is_sequence};

pub(crate) fn orphan_objection(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for cls in ix.model_classes() {
        if !is_sequence(d, cls.id) {
            continue;
        }
        for at in ix.own_routines(cls.id) {
            let c = ix.cst(at);
            for fork in subtree(c, at.node).into_iter().filter(|&n| is_join_none(c, n)) {
                for thread in block_stmts(c, fork) {
                    let (mut raise, mut drop) = (None, false);
                    for k in calls_in(c, thread) {
                        match call_name(c, k) {
                            "raise_objection" => raise = c.first_tok(k),
                            "drop_objection" => drop = true,
                            _ => {}
                        }
                    }
                    let Some(t) = raise.filter(|_| !drop) else { continue };
                    s.add(
                        "UVM-OBJ-004",
                        "OrphanSubsequenceObjectionHang",
                        Severity::Error,
                        format!(
                            "Asynchronous background process spawned via fork..join_none in sequence '{}' raises an objection without a local drop_objection() call. \
                             When the parent sequence finishes, this orphan objection will remain active, causing a phase timeout simulation hang.",
                            cls.name
                        ),
                        c.loc(t),
                    );
                }
            }
        }
    }
}
