//! UVM-SEQ-016 (SequencerFromConfigDb): Flags uvm_config_db/uvm_resource_db
//! get() calls in a sequence that fetch a sequencer handle.

use dv_model::Design;
use crate::support::routine_bodies::{bodies, calls_in, Calls};
use crate::support::class_roles::{role, Ctx, Index, Role};
use crate::syntax::Cst;
use crate::{Severity, Sink};

pub(crate) fn sequencer_from_config_db(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for (cls, m, c, body) in bodies(d, &ix) {
        if role(d, cls) != Role::Sequence {
            continue;
        }
        let ctx = Ctx { d, cls, m: Some(m), td: Some(&ix.typedefs) };
        let calls = Calls::new(m);
        for call in calls_in(c, body) {
            if c.callee_name(call) != "get" {
                continue;
            }
            let owner = calls.owner(c, call);
            if owner != "uvm_config_db" && owner != "uvm_resource_db" {
                continue;
            }
            let args = c.call_args(call);
            let Some(t) = args.get(3).and_then(|a| c.arg_expr(*a)) else { continue };
            let Some(k) = ctx.class_of(c, t) else { continue };
            if role(d, k) != Role::Sequencer {
                continue;
            }
            out.add(
                "UVM-SEQ-016",
                "SequencerFromConfigDb",
                Severity::Warning,
                format!(
                    "Sequence '{}' looks up sequencer '{}' in uvm_config_db. Every sequence that does this repeats the plumbing and ties stimulus to hierarchy strings. Run it \
on a virtual sequencer (p_sequencer) or have the test hand it the handles.",
                    cls.name, k.name
                ),
                c.loc_of(call),
            );
        }
    }
}
