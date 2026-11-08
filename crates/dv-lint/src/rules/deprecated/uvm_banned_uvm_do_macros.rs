//! UVM-BAN-002 (UvmBannedUvmDoMacros): Discourages black-box `uvm_do macros in
//! sequence bodies.

use crate::syntax::Cst;
use crate::Sink;
use crate::support::source_layout::{banned_macros};

pub(crate) fn banned_do_macros(c: &Cst, s: &mut Sink) {
    banned_macros(c, s, false);
}
