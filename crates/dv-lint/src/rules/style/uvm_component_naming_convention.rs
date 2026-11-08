//! SV-NAM-001 (UvmComponentNamingConvention): Enforces standard naming
//! conventions for UVM components: classes derived from uvm_driver must end
//! with '_driver', uvm_monitor with '_monitor', uvm_agent with '_agent',
//! uvm_env with '_env', and uvm_test with '_test'.

use dv_model::Design;
use crate::{Severity, Sink, derives_from};

pub(crate) fn uvm_component_naming_convention(d: &Design, out: &mut Sink) {
    const CHECKS: [(&str, &str, &str); 5] = [
        ("uvm_driver", "_driver", "driver"),
        ("uvm_monitor", "_monitor", "monitor"),
        ("uvm_agent", "_agent", "agent"),
        ("uvm_env", "_env", "environment"),
        ("uvm_test", "_test", "test"),
    ];
    for cls in d.classes() {
        if cls.is_parameterized || cls.name.starts_with("uvm_") || cls.name.is_empty() {
            continue;
        }
        for (base, suffix, role) in CHECKS {
            if derives_from(d, Some(cls.id), base) {
                if !cls.name.ends_with(suffix) {
                    out.add(
                        "SV-NAM-001",
                        "UvmComponentNamingConvention",
                        Severity::Warning,
                        format!(
                            "UVM {} class '{}' derived from '{}' does not end with required suffix '{}'. Following uniform naming conventions is essential for VIP maintainability and automated testbench reflection.",
                            role, cls.name, base, suffix
                        ),
                        cls.loc,
                    );
                }
                break;
            }
        }
    }
}
