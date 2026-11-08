//! UVM-SEQ-009 (SequenceBodyWithoutStartItem): Flags calls to finish_item()
//! inside uvm_sequence::body() that are not preceded by a corresponding
//! start_item().

use std::collections::HashSet;
use dv_model::Design;
use sv_syntax::{NodeId, SyntaxKind};
use crate::support::uvm_syntax::{
    arg_list, body_items, params, stmt_expr,
};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::uvm_classes::{Ix, block_stmts, is_name_chain, sequence_bodies};

fn start_finish(c: &Cst, cls: &str, n: NodeId, started: &mut HashSet<String>, s: &mut Sink) {
    match c.kind(n) {
        ExprStmt => {
            let Some(e) = stmt_expr(c, n).filter(|&e| c.kind(e) == CallExpr) else { return };
            let Some(left) = c.tree.children(e).next() else { return };
            let method = match c.kind(left) {
                NameExpr => c.tree.bare_text(left),
                MemberExpr if !is_name_chain(c, left) => c.last_text(left).to_string(),
                _ => String::new(),
            };
            let arg = arg_list(c, e)
                .and_then(|l| params(c, l).first().copied().flatten())
                .map(|a| c.trimmed(a))
                .unwrap_or_else(|| "req".into());
            if method == "start_item" {
                started.insert(arg);
            } else if method == "finish_item" && !started.contains(&arg) {
                let Some(t) = c.first_tok(e) else { return };
                s.add(
                    "UVM-SEQ-009",
                    "SequenceBodyWithoutStartItem",
                    Severity::Error,
                    format!(
                        "Call to 'finish_item({arg})' in sequence '{cls}' body() is not preceded by a matching 'start_item({arg})'. \
                         In UVM sequence arbitration, start_item() requests sequencer grant and locks driver synchronization \
                         before finish_item() sends the transaction. Calling finish_item() without start_item() causes fatal sequencer errors."
                    ),
                    c.loc(t),
                );
            }
        }
        SeqBlock => {
            for i in block_stmts(c, n) {
                start_finish(c, cls, i, started, s);
            }
        }
        _ => {}
    }
}

pub(crate) fn body_without_start_item(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for c in csts {
        for (cls, f) in sequence_bodies(&ix, c) {
            let mut started = HashSet::new();
            for i in body_items(c, f) {
                start_finish(c, &cls, i, &mut started, s);
            }
        }
    }
}
