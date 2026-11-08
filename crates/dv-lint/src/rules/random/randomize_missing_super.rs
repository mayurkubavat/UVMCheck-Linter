//! SV-RND-003 (RandomizeMissingSuper): Overridden pre_randomize() and
//! post_randomize() callback methods must invoke super.pre_randomize() or
//! super.post_randomize() to prevent bypassing base-class transaction setup and
//! validation routines.

use crate::{find_method, Severity, Sink};
use dv_model::Design;

pub(crate) fn randomize_missing_super(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || cls.base.is_none() {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.is_builtin {
                continue;
            }
            if sub.name != "pre_randomize" && sub.name != "post_randomize" {
                continue;
            }
            let above = d.ancestors(cls.id).any(|a| find_method(a, &sub.name).is_some_and(|m| !m.is_builtin));
            if !above {
                continue;
            }
            if sub.calls.iter().any(|c| !c.is_system && c.name == sub.name && c.via_super) {
                continue;
            }
            out.add(
                "SV-RND-003",
                "RandomizeMissingSuper",
                Severity::Warning,
                format!(
                    "Overridden randomization callback '{}' in class '{}' does not invoke 'super.{}()'. \
Unlike UVM phases, SV pre/post_randomize callbacks do not auto-chain up inheritance; \
omitting super bypasses base-class payload, CRC, and sequence updates.",
                    sub.name, cls.name, sub.name
                ),
                sub.loc,
            );
        }
    }
}
