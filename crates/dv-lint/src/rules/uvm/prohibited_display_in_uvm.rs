//! UVM-LOG-002 (ProhibitedDisplayInUvm): Disallow direct
//! $display/$write/$strobe calls inside UVM classes (components, objects,
//! sequences).

use dv_model::Design;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::uvm_classes::{Ix, call_name, calls_in, derived, is_component, is_sequence};

const DISPLAYS: [&str; 12] = [
    "$display", "$displayh", "$displayb", "$displayo", "$write", "$writeh", "$writeb", "$writeo", "$strobe", "$strobeh",
    "$strobeb", "$strobeo",
];

pub(crate) fn display_in_uvm(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for cls in ix.model_classes() {
        let id = cls.id;
        let uvm = derived(d, id, "uvm_report_object")
            || is_component(d, id)
            || is_sequence(d, id)
            || derived(d, id, "uvm_object");
        if !uvm {
            continue;
        }
        for at in ix.own_routines(cls.id) {
            let c = ix.cst(at);
            for n in calls_in(c, at.node) {
                let name = call_name(c, n);
                if !DISPLAYS.contains(&name) {
                    continue;
                }
                let Some(t) = c.first_tok(n) else { continue };
                s.add(
                    "UVM-LOG-002",
                    "ProhibitedDisplayInUvm",
                    Severity::Warning,
                    format!(
                        "Direct system task '{name}' called in UVM class '{}'. Direct display tasks bypass UVM report catchers, severity filtering (+uvm_set_verbosity), and regression log routing. Use `uvm_info or `uvm_error instead.",
                        cls.name
                    ),
                    c.loc(t),
                );
            }
        }
    }
}
