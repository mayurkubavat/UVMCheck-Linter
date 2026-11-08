//! UVM-CFG-004 (ConfigDbGetUnchecked): Flags calls to
//! 'uvm_config_db#(...)::get()' where the boolean return status is ignored
//! without being checked in an 'if' condition or assertion.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::calls::{callee, statement_calls};

pub(crate) fn config_db_get_unchecked(c: &Cst, s: &mut Sink) {
    for call in statement_calls(c) {
        let Some(f) = callee(c, call) else { continue };
        if c.last_text(f) != "get" || !c.text(f).contains("uvm_config_db") {
            continue;
        }
        let Some(t) = c.first_tok(call) else { continue };
        s.add(
            "UVM-CFG-004",
            "ConfigDbGetUnchecked",
            Severity::Warning,
            "Calling 'uvm_config_db::get()' without checking its return boolean status risks silent uninitialized state or null-pointer dereferences if the field is not found. Check the return value in an 'if' statement (e.g. 'if (!uvm_config_db#(...)::get(...)) `uvm_fatal(...)').".to_string(),
            c.loc(t),
        );
    }
}
