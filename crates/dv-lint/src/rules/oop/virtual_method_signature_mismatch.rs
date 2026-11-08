//! SV-OOP-015 (VirtualMethodSignatureMismatch): Virtual method overrides in
//! derived classes must match the parameter count and signature of the base
//! virtual method.

use dv_model::Design;
use crate::{Severity, Sink, find_method};

pub(crate) fn virtual_method_signature_mismatch(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        let Some(base) = cls.base else { continue };
        let base = d.class(base);
        for sub in &cls.methods {
            if sub.is_extern || sub.name == "new" {
                continue;
            }
            let Some(bs) = find_method(base, &sub.name) else {
                continue;
            };
            if !bs.declared_virtual {
                continue;
            }
            let (da, ba) = (sub.arguments.len(), bs.arguments.len());
            if da != ba {
                out.add(
                    "SV-OOP-015",
                    "VirtualMethodSignatureMismatch",
                    Severity::Warning,
                    format!(
                        "Method '{}' in derived class '{}' defines {} parameter(s), but overrides virtual method in base class '{}' defined with {} parameter(s). In SystemVerilog OOP \
(IEEE 1800 § 8.20), virtual method overrides must match base signature parameter counts; \
mismatching arguments causes method hiding instead of polymorphic override.",
                        sub.name, cls.name, da, base.name, ba
                    ),
                    sub.loc,
                );
            }
        }
    }
}
