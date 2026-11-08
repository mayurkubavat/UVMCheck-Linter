//! SV-RND-001 (UncheckedRandomize): The return value of randomize() must be
//! checked.

use crate::support::class_items::{call_name, strip_casts};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn unchecked_randomize(c: &Cst, s: &mut Sink) {
    for st in c.nodes(ExprStmt) {
        let Some(e) = c.tree.children(st).next() else { continue };
        let e = strip_casts(c, e);
        if call_name(c, e) != Some("randomize") || !crate::support::sema_facts::ast_ok(c, e) {
            continue;
        }
        let Some(at) = c.first_tok(e) else { continue };
        s.add(
            "SV-RND-001",
            "UncheckedRandomize",
            Severity::Error,
            "Return value of 'randomize()' is discarded or cast to void. If constraint solving fails, \
             simulation proceeds silently with corrupt, unconstrained, or stale transaction data. \
             Check the return value with 'if (!obj.randomize()) `uvm_fatal(...)'. Avoid 'assert(obj.randomize())': turning assertions off skips the call (SV-RND-024)."
                .into(),
            c.loc(at),
        );
    }
}
