//! SV-CON-008 (MutexTryGetWithoutCheck): Flags non-blocking semaphore or mutex
//! try_get() calls whose return status is ignored, which risks threads
//! executing critical sections without holding the lock.

use sv_syntax::SyntaxKind;
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::procedural::{add_n, start, stmt_call};

pub(crate) fn try_get_unchecked(c: &Cst, s: &mut Sink) {
    for st in c.nodes(ExprStmt) {
        let Some(call) = stmt_call(c, st) else { continue };
        let Some((_, name, recv)) = call_parts(c, call) else { continue };
        if name != "try_get" {
            continue;
        }
        let Some(r) = recv else { continue };
        let ty = expr_type(c, r).unwrap_or_default();
        let mut mutex = ty.contains("semaphore") || ty.contains("mutex") || ty.contains("lock");
        let target = match c.kind(r) {
            NameExpr => c.first_tok(r).map(|t| c.text_of(t).to_string()).unwrap_or_default(),
            MemberExpr => c.last_text(r).to_string(),
            _ => slang_trimmed(c, r),
        };
        if target.contains("sem") || target.contains("mutex") || target.contains("lock") {
            mutex = true;
        }
        if !mutex {
            continue;
        }
        if let Some(loc) = start(c, call) {
            add_n(
                s,
                times(c, st),
                "SV-CON-008",
                "MutexTryGetWithoutCheck",
                Severity::Warning,
                format!(
                    "Non-blocking semaphore acquisition '{target}.try_get()' called without verifying its return code. \
                     In SystemVerilog (IEEE 1800 § 15.3), try_get() returns 0 when the requested keys are unavailable. \
                     Discarding the return value causes the thread to execute critical section code without acquiring the mutual exclusion lock, \
                     risking catastrophic race conditions and data corruption. Wrap the call in 'if ({target}.try_get(...))'."
                ),
                loc,
            );
        }
    }
}
