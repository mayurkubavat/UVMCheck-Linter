//! SV-IPC-001 (TryPutIgnoredReturn): Non-blocking IPC routines (try_put,
//! try_get, try_peek) return a boolean status.

use crate::support::procedural::{add_n, start, stmt_call};
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn try_put_ignored(c: &Cst, s: &mut Sink) {
    for st in c.nodes(ExprStmt) {
        let Some(call) = stmt_call(c, st) else { continue };
        let Some((_, name, recv)) = call_parts(c, call) else { continue };
        if !matches!(name.as_str(), "try_put" | "try_get" | "try_peek") {
            continue;
        }
        if name == "try_get" && recv.is_some_and(|r| expr_type(c, r).is_some_and(|t| t.contains("semaphore"))) {
            continue;
        }
        if let Some(loc) = start(c, call) {
            add_n(
                s,
                times(c, st),
                "SV-IPC-001",
                "TryPutIgnoredReturn",
                Severity::Warning,
                format!(
                    "The return value of non-blocking IPC method '{name}()' is ignored or cast to void. \
                     Non-blocking IPC calls return 1 on success and 0 when the FIFO or mailbox is full or empty. \
                     Discarding the return value causes transactions to be silently lost without warning. \
                     Check the return value with 'if (!fifo.{name}(...))' or use blocking 'put()'."
                ),
                loc,
            );
        }
    }
}
