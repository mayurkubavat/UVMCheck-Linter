//! UVM-CFG-005 (ConfigDbCrossComponentResolver): Performs cross-component
//! semantic resolution on uvm_config_db calls across the testbench hierarchy
//! DAG to detect configuration type mismatches between set() and get() calls.

use crate::support::config_db_calls::config_db_calls;
use crate::syntax::Cst;
use crate::{Severity, Sink};

pub(crate) fn config_db_cross_component(_: &dv_model::Design, csts: &[Cst], out: &mut Sink) {
    let calls = config_db_calls(csts);
    // Only literal field names name a field: two wrappers forwarding a
    // `field_name` variable are not the same configuration.
    for g in calls.iter().filter(|k| !k.set && k.literal) {
        for s in calls.iter().filter(|k| k.set && k.literal && k.field == g.field && k.ty != g.ty) {
            // A type declared in the calling class (`CFG_T`, a type parameter
            // of dv_base_agent): which type it is depends on the
            // specialization, so the two cannot be told apart. The C++ rule
            // skips the same pairs.
            if class_scoped(&g.ty, &g.class) || class_scoped(&s.ty, &s.class) {
                continue;
            }
            let msg = format!(
                "Cross-component 'uvm_config_db' type mismatch for field '{}': configured in class '{}' with type '{}', but retrieved in class '{}' with incompatible type '{}'. Mismatched type parameters break runtime config lookup and leave fields uninitialized.",
                g.field, s.class, s.ty, g.class, g.ty
            );
            for _ in 0..g.visits * s.visits {
                out.add("UVM-CFG-005", "ConfigDbCrossComponentResolver", Severity::Error, msg.clone(), g.loc);
            }
        }
    }
}

/// `ty` is a type declared inside class `class` (`pkg::cls::CFG_T`).
fn class_scoped(ty: &str, class: &str) -> bool {
    !class.is_empty() && (ty.starts_with(&format!("{class}::")) || ty.contains(&format!("::{class}::")))
}
