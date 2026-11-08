//! UVM-ARC-001 (LayerDependency): Flags classes that depend on a higher
//! testbench layer (item < sequence < agent < env < test), such as a sequence
//! that references a scoreboard or an agent that references the env (ArchUnit
//! layer rules).

use crate::support::class_index::*;
use crate::support::class_metrics::each_class;
use crate::syntax::Cst;
use crate::{Severity, Sink};

fn layer_of(r: Role) -> Option<usize> {
    match r {
        Role::Item => Some(0),
        Role::Sequence => Some(1),
        Role::Driver | Role::Monitor | Role::Sequencer | Role::Agent | Role::Subscriber => Some(2),
        Role::Scoreboard | Role::Env => Some(3),
        Role::Test => Some(4),
        _ => None,
    }
}

fn layer_name(l: usize) -> &'static str {
    ["sequence item", "sequence", "agent", "env", "test"].get(l).copied().unwrap_or("?")
}

pub(crate) fn layer_dependency(c: &Cst, s: &mut Sink) {
    each_class(c, |classes, cm| {
        let role = classes.role(cm.cls);
        let Some(mine) = layer_of(role) else { return };
        let cls = &classes.list[cm.cls];
        for &dep in &cm.coupled {
            let dr = classes.role(dep);
            let Some(theirs) = layer_of(dr) else { continue };
            if theirs <= mine {
                continue;
            }
            if role == Role::Sequence && dr == Role::Sequencer {
                continue;
            }
            s.add(
                "UVM-ARC-001",
                "LayerDependency",
                Severity::Warning,
                format!(
                    "{} '{}' ({} layer) depends on {} '{}' ({} layer). Lower layers must not know about the \
                     layers that use them, or the {} \
                     code cannot be reused in another env. Pass data down through config objects, \
                     TLM ports or callbacks instead.",
                    role.name(),
                    cls.name,
                    layer_name(mine),
                    dr.name(),
                    classes.list[dep].name,
                    layer_name(theirs),
                    layer_name(mine)
                ),
                c.loc(cls.tok),
            );
        }
    });
}
