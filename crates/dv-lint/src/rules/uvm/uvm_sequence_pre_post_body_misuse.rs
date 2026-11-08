//! UVM-SEQ-002 (UvmSequencePrePostBodyMisuse): pre_body() and post_body()
//! callbacks in uvm_sequence are explicitly bypassed when the sequence is
//! executed as a child sequence via `uvm_do or `uvm_do_with.

use dv_model::{Design, UvmKind};
use crate::{Severity, Sink};

pub(crate) fn uvm_sequence_pre_post_body_misuse(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || cls.uvm != UvmKind::Sequence {
            continue;
        }
        for m in &cls.methods {
            if m.is_extern || (m.name != "pre_body" && m.name != "post_body") {
                continue;
            }
            out.add(
                "UVM-SEQ-002",
                "UvmSequencePrePostBodyMisuse",
                Severity::Warning,
                format!(
                    "Sequence '{}' overrides lifecycle callback '{}()'. \
In UVM sequence architecture, pre_body() and post_body() are explicitly skipped when a sequence is executed as a child sequence \
via `uvm_do or `uvm_do_with. Critical setup or cleanup logic will silently fail to execute when nested. \
Migrate logic to pre_start() / post_start() (which execute in all modes) or body().",
                    cls.name, m.name
                ),
                m.loc,
            );
        }
    }
}
