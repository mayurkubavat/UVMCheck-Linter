//! UVM-SEQ-015 (UnusedSequenceItem): Flags a sequence item that no other class
//! in the compilation names, when the run also contains a test.

use dv_model::Design;
use crate::syntax::Cst;
use crate::Sink;
use crate::support::uvm_classes::{Role, unused};

pub(crate) fn unused_sequence_item(d: &Design, csts: &[Cst], s: &mut Sink) {
    unused(d, csts, s, Role::Item, "UVM-SEQ-015", "UnusedSequenceItem", "Sequence item");
}
