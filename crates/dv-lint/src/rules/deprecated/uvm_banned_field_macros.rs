//! UVM-BAN-001 (UvmBannedFieldMacros): Bans the `uvm_field_* macro suite.

use crate::support::source_layout::banned_macros;
use crate::syntax::Cst;
use crate::Sink;

pub(crate) fn banned_field_macros(c: &Cst, s: &mut Sink) {
    banned_macros(c, s, true);
}
