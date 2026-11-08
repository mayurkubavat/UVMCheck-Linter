//! UVM-CFG-001 (UvmConfigDbTypeMismatch): The type parameter in
//! uvm_config_db#(T)::get must match the type parameter used in the
//! corresponding set call for the same field name.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::config_db_calls::{CfgCall, config_db_calls};

pub(crate) fn config_db_type_mismatch(_: &dv_model::Design, csts: &[Cst], out: &mut Sink) {
    let calls = config_db_calls(csts);
    // The set the AST walk meets last for each field.
    let mut sets: std::collections::HashMap<&str, &CfgCall> = std::collections::HashMap::new();
    for k in calls.iter().filter(|k| k.set && k.visits > 0) {
        sets.insert(&k.field, k);
    }
    for g in calls.iter().filter(|k| !k.set) {
        let Some(s) = sets.get(g.field.as_str()) else { continue };
        if s.ty == g.ty {
            continue;
        }
        for _ in 0..g.visits {
            out.add(
                "UVM-CFG-001",
                "UvmConfigDbTypeMismatch",
                Severity::Error,
                format!(
                    "Type mismatch in 'uvm_config_db' for field '{}': value was configured as type '{}', but is retrieved as type '{}'. Mismatched types prevent configuration retrieval at runtime.",
                    g.field, s.ty, g.ty
                ),
                g.loc,
            );
        }
    }
}
