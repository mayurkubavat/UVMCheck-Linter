//! UVM-SEQ-001 (UvmSequenceDirectVifAccess): Sequences and sequence items must
//! not directly declare or drive virtual interface signals.

use crate::{Severity, Sink};
use dv_model::{Design, UvmKind};

pub(crate) fn uvm_sequence_direct_vif_access(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        let is_seq = cls.uvm == UvmKind::Sequence;
        if !is_seq && cls.uvm != UvmKind::SequenceItem {
            continue;
        }
        let kind = if is_seq { "Sequence" } else { "Sequence item" };
        for p in &cls.properties {
            if !p.is_virtual_interface {
                continue;
            }
            out.add(
                "UVM-SEQ-001",
                "UvmSequenceDirectVifAccess",
                Severity::Warning,
                format!(
                    "{kind} class '{}' directly declares virtual interface property '{}'. Sequences must remain protocol-abstract and transaction-level; \
pin-level driving is the responsibility of the driver component.",
                    cls.name, p.name
                ),
                p.loc,
            );
        }
        for sub in &cls.methods {
            if sub.is_extern {
                continue;
            }
            for v in &sub.locals {
                if !v.is_virtual_interface {
                    continue;
                }
                out.add(
                    "UVM-SEQ-001",
                    "UvmSequenceDirectVifAccess",
                    Severity::Warning,
                    format!(
                        "Method '{}' in sequence class '{}' declares virtual interface variable '{}'. Pin-level driving must be confined to drivers.",
                        sub.name, cls.name, v.name
                    ),
                    v.loc,
                );
            }
        }
    }
}
