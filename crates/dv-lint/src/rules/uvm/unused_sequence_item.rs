//! UVM-SEQ-015 (UnusedSequenceItem): Flags a sequence item that no other class
//! in the compilation names, when the run also contains a test.

use crate::support::uvm_classes::{unused, Role};
use crate::syntax::Cst;
use crate::Sink;
use dv_model::Design;

pub(crate) fn unused_sequence_item(d: &Design, csts: &[Cst], s: &mut Sink) {
    unused(d, csts, s, Role::Item, "UVM-SEQ-015", "UnusedSequenceItem", "Sequence item");
}
