//! UVM-VSEQ-001 (VirtualSequencerWiringChecker): Statically verifies that
//! sub-sequencer handles declared within a virtual sequencer are fully wired
//! and assigned in connect_phase across the verification environment.

use dv_model::Design;
use crate::{derives_from, Severity, Sink};
use crate::support::connect_phase::{connect_phase_assignments};

pub(crate) fn virtual_sequencer_wiring_checker(d: &Design, out: &mut Sink) {
    let mut fields = Vec::new();
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        let vseqr = cls.name.contains("vseqr")
            || cls.name.contains("virtual_sequencer")
            || derives_from(d, Some(cls.id), "uvm_sequencer");
        if !vseqr {
            continue;
        }
        for p in &cls.properties {
            let sub = p.ty.contains("sequencer")
                || p.name.contains("_seqr")
                || p.name.contains("sequencer");
            if sub && !derives_from(d, p.type_class, "uvm_phase") {
                fields.push((cls, p));
            }
        }
    }
    if fields.is_empty() {
        return;
    }
    let assignments = connect_phase_assignments(d);
    for (owner, p) in fields {
        if assignments.iter().any(|s| s.contains(p.name.as_str())) {
            continue;
        }
        out.add(
            "UVM-VSEQ-001",
            "VirtualSequencerWiringChecker",
            Severity::Warning,
            format!(
                "Sub-sequencer handle '{0}' in virtual sequencer '{1}' is declared but never wired in any connect_phase. \
Virtual sequences attempting to access p_sequencer.{0} will crash with a null-pointer dereference at runtime.",
                p.name, owner.name
            ),
            p.loc,
        );
    }
}
