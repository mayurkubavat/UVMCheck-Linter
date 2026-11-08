//! SV-STY-001 (ClassMemberPrefix): Enforces that member properties of
//! uvm_component classes begin with 'm_' prefix to distinguish local member
//! state from arguments and parameters.

use crate::{Severity, Sink};
use dv_model::Design;

pub(crate) fn class_member_prefix(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || !cls.uvm.is_component() || cls.name.starts_with("uvm_") {
            continue;
        }
        for p in &cls.properties {
            if p.is_const || p.is_static || p.name == "type_name" || p.name == "type_id" {
                continue;
            }
            if !p.name.starts_with("m_") {
                out.add(
                    "SV-STY-001",
                    "ClassMemberPrefix",
                    Severity::Info,
                    format!(
                        "Member property '{}' in class '{}' should begin with 'm_' prefix (e.g. 'm_{}') to distinguish internal state from local arguments.",
                        p.name, cls.name, p.name
                    ),
                    p.loc,
                );
            }
        }
    }
}
