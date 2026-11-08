//! UVM-SEQ-003 (UvmPSequencerAbuse): Using `uvm_declare_p_sequencer or
//! accessing p_sequencer violates the Law of Demeter by tightly coupling the
//! sequence data tier to physical sequencer component internals.

use dv_model::{Design, UvmKind};
use crate::{find_property, Severity, Sink};

pub(crate) fn uvm_p_sequencer_abuse(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || cls.uvm != UvmKind::Sequence {
            continue;
        }
        let Some(p) = find_property(cls, "p_sequencer") else {
            continue;
        };
        out.add(
            "UVM-SEQ-003",
            "UvmPSequencerAbuse",
            Severity::Warning,
            format!(
                "Sequence '{}' declares 'p_sequencer' via `uvm_declare_p_sequencer. \
This violates the Law of Demeter and architectural layering by tightly binding sequence scenario generation \
to physical testbench agent/sequencer topology. If sequencer topology or naming changes, sequences break. \
Decouple sequences by passing configuration parameters down through sequence properties or uvm_config_db.",
                cls.name
            ),
            p.loc,
        );
    }
}
