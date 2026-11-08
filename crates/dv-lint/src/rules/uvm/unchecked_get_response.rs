//! UVM-SEQ-006 (UncheckedGetResponse): Flags calls to get_response() in UVM
//! sequences.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;
use crate::support::uvm_syntax::{callee, decl_name, enclosing_class};

pub(crate) fn unchecked_get_response(c: &Cst, s: &mut Sink) {
    for n in c.nodes(CallExpr) {
        if callee(c, n) != "get_response" {
            continue;
        }
        let target = enclosing_class(c, n).map(|k| decl_name(c, k)).filter(|x| !x.is_empty());
        let target = target.unwrap_or_else(|| "sequence".into());
        let Some(at) = c.first_tok(n) else { continue };
        s.add(
            "UVM-SEQ-006",
            "UncheckedGetResponse",
            Severity::Warning,
            format!(
                "UVM sequence class '{target}' calls 'get_response()'. In UVM, calling get_response() blocks the sequence thread until the driver provides a response via 'seq_item_port.item_done(rsp)' or 'put_response(rsp)'. If the driver is not implemented to return response items, the sequence will hang indefinitely."
            ),
            c.loc(at),
        );
    }
}
