//! UVM-BAN-001 (UvmBannedFieldMacros): Bans the `uvm_field_* macro suite.

use crate::syntax::Cst;
use crate::Sink;
use crate::support::source_layout::{banned_macros};

pub(crate) fn banned_field_macros(c: &Cst, s: &mut Sink) {
    banned_macros(c, s, true);
}
