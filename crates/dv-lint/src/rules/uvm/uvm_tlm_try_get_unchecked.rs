//! UVM-TLM-002 (UvmTlmTryGetUnchecked): Flags calls to non-blocking TLM methods
//! (try_get(), try_peek(), try_put()) where the boolean return status is
//! ignored.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::calls::{callee, statement_calls};

pub(crate) fn tlm_try_unchecked(c: &Cst, s: &mut Sink) {
    for call in statement_calls(c) {
        let Some(f) = callee(c, call) else { continue };
        let m = c.last_text(f);
        if !matches!(m, "try_get" | "try_peek" | "try_put") {
            continue;
        }
        let Some(t) = c.first_tok(call) else { continue };
        s.add(
            "UVM-TLM-002",
            "UvmTlmTryGetUnchecked",
            Severity::Warning,
            format!(
                "Calling non-blocking TLM method '.{m}()' without checking its return boolean status causes silent transaction drops when the channel is empty or full. Check the return status in an 'if' statement (e.g. 'if (!port.{m}(tr)) ...')."
            ),
            c.loc(t),
        );
    }
}
