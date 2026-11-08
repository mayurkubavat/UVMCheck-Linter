//! UVM-SEQ-014 (UnusedSequence): Flags a sequence that no other class in the
//! compilation names, when the run also contains a test.

use dv_model::Design;
use crate::syntax::Cst;
use crate::Sink;
use crate::support::uvm_classes::{Role, unused};

pub(crate) fn unused_sequence(d: &Design, csts: &[Cst], s: &mut Sink) {
    unused(d, csts, s, Role::Sequence, "UVM-SEQ-014", "UnusedSequence", "Sequence");
}
