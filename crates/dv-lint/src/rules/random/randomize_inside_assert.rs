//! SV-RND-024 (RandomizeInsideAssert): Flags randomize() used as the condition
//! of an immediate assert or assume.

use crate::support::class_items::{call_name, kids, op, strip_casts, user_code};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

fn calls_randomize(c: &Cst, e: NodeId) -> bool {
    let e = strip_casts(c, e);
    match c.kind(e) {
        CallExpr => call_name(c, e) == Some("randomize"),
        BinaryExpr if matches!(op(c, e), "&&" | "||") => kids(c, e).into_iter().take(2).any(|x| calls_randomize(c, x)),
        _ => false,
    }
}

pub(crate) fn randomize_in_assert(c: &Cst, s: &mut Sink) {
    {
        for st in c.nodes(AssertStmt) {
            if !matches!(op(c, st), "assert" | "assume") || !user_code(c, st) {
                continue;
            }
            let Some(cond) = c.items(st).into_iter().next() else { continue };
            if !calls_randomize(c, cond) {
                continue;
            }
            let Some(at) = c.first_tok(st) else { continue };
            s.add(
                "SV-RND-024",
                "RandomizeInsideAssert",
                Severity::Warning,
                "randomize() is called inside an assertion. When assertions are turned off \
                 ($assertoff or a simulator switch) the call is skipped and the object keeps its old \
                 values. Call it on its own: 'if (!obj.randomize()) `uvm_fatal(...)'."
                    .into(),
                c.loc(at),
            );
        }
    }
}
