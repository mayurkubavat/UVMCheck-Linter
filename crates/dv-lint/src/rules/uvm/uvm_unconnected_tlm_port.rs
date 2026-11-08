//! UVM-TLM-001 (UvmUnconnectedTlmPort): Mandatory TLM ports and exports
//! declared in uvm_component must be connected in connect_phase to prevent
//! runtime fatal errors or null-pointer dereferences.

use crate::{Severity, Sink};
use dv_model::Design;

pub(crate) fn uvm_unconnected_tlm_port(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || !cls.uvm.is_component() {
            continue;
        }
        let ports: Vec<_> =
            cls.properties.iter().filter(|p| p.ty.contains("_port") || p.ty.contains("_export")).collect();
        if ports.is_empty() {
            continue;
        }
        let connected: Vec<&str> = cls
            .methods
            .iter()
            .filter(|m| !m.is_extern && !m.is_builtin)
            .flat_map(|m| m.calls.iter())
            .filter(|c| c.name == "connect" && !c.receiver.is_empty())
            .map(|c| c.receiver.as_str())
            .collect();
        for p in ports {
            if connected.iter().any(|r| r.ends_with(p.name.as_str())) {
                continue;
            }
            out.add(
                "UVM-TLM-001",
                "UvmUnconnectedTlmPort",
                Severity::Warning,
                format!(
                    "TLM port/export '{}' in component '{}' is declared but never connected via .connect(...). \
Unconnected TLM ports can cause simulation fatal errors upon transaction transfer.",
                    p.name, cls.name
                ),
                p.loc,
            );
        }
    }
}
