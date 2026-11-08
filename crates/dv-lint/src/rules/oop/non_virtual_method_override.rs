//! SV-OOP-001 (NonVirtualMethodOverride): Overriding a non-virtual method in a
//! derived class breaks polymorphism when accessed via a base class handle.

use dv_model::Design;
use crate::{Severity, Sink, ancestors, find_method};

pub(crate) fn non_virtual_method_override(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || cls.base.is_none() {
            continue;
        }
        let chain = ancestors(d, cls);
        for sub in &cls.methods {
            if sub.is_extern || sub.name == "new" || sub.is_builtin {
                continue;
            }
            for &a in &chain {
                let anc = d.class(a);
                let Some(bm) = find_method(anc, &sub.name) else {
                    continue;
                };
                if bm.is_builtin {
                    break;
                }
                if !bm.is_virtual {
                    let f = out.add(
                        "SV-OOP-001",
                        "NonVirtualMethodOverride",
                        Severity::Warning,
                        format!("Method '{}' in class '{}' overrides non-virtual method in base class '{}'.", sub.name, cls.name, anc.name),
                        sub.loc,
                    );
                    f.related
                        .push((bm.loc, format!("non-virtual method in '{}'", anc.name)));
                }
                break;
            }
        }
    }
}
