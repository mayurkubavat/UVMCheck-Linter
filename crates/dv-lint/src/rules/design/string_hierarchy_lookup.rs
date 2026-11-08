//! UVM-ARC-004 (StringHierarchyLookup): Flags uvm_root/uvm_component find(),
//! find_all() and lookup() calls outside tests and end_of_elaboration_phase.

use crate::support::class_index::{calls, normalized, Classes, Role};
use crate::support::statements::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};

pub(crate) fn string_hierarchy_lookup(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    for i in classes.user() {
        let role = classes.role(i);
        if role == Role::Test {
            continue;
        }
        let cls = &classes.list[i];
        let g = crate::support::class_metrics::globals(c);
        for m in &cls.methods {
            if m.name == "end_of_elaboration_phase" {
                continue;
            }
            let Some(body) = m.body else { continue };
            let env = crate::support::class_metrics::Env::new(c, &classes, Some(i), Some(m.decl), &g);
            for call in calls(c, body) {
                let name = call_name(c, call).unwrap_or("");
                if !matches!(name, "find" | "find_all" | "lookup") {
                    continue;
                }
                if !env.callee(call).is_some_and(|(k, _)| classes.list[k].library) {
                    continue;
                }
                s.add(
                    "UVM-ARC-004",
                    "StringHierarchyLookup",
                    Severity::Warning,
                    format!(
                        "{} '{}' looks up components by path with '{}' in {}(). The dependency is invisible and the lookup returns \
                         null as soon as the hierarchy is renamed. Get the handle through config_db, \
                         a config object or a TLM connection.",
                        role.name(),
                        cls.name,
                        normalized(c, call),
                        m.name
                    ),
                    start(c, call),
                );
            }
        }
    }
}
