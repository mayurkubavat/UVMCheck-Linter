//! UVM-BAN-003 (DirectDoMethodCall): Flags direct calls to do_copy, do_compare,
//! do_print, do_pack, do_unpack and do_record.

use std::collections::HashMap;
use dv_model::{Class, Design};
use crate::{Severity, Sink};
use crate::support::uvm_kinds::{is_object};

pub(crate) fn direct_do_method_call(d: &Design, out: &mut Sink) {
    const PUBLIC: [(&str, &str); 6] = [
        ("do_copy", "copy()"),
        ("do_compare", "compare()"),
        ("do_print", "print() or sprint()"),
        ("do_pack", "pack()"),
        ("do_unpack", "unpack()"),
        ("do_record", "record()"),
    ];
    // Callee classes by name: the first class of each name, as findClass.
    let mut by_name: HashMap<&str, &Class> = HashMap::new();
    for c in d.classes() {
        by_name.entry(c.name.as_str()).or_insert(c);
    }
    for cls in d.classes() {
        for sub in &cls.methods {
            for call in &sub.calls {
                if call.is_system || call.via_super {
                    continue;
                }
                let Some(&(_, public)) = PUBLIC.iter().find(|(n, _)| *n == call.name) else { continue };
                let Some(owner) = by_name.get(call.callee_scope.as_str()) else { continue };
                if !is_object(d, owner) {
                    continue;
                }
                out.add(
                    "UVM-BAN-003",
                    "DirectDoMethodCall",
                    Severity::Warning,
                    format!(
                        "Direct call to {}(). The do_* methods are hooks that the UVM \
policy classes call from {public}; calling them directly skips the \
policy object (comparer, printer, packer, recorder) and the field automation. Call {public} instead.",
                        call.name
                    ),
                    call.loc,
                );
            }
        }
    }
}
