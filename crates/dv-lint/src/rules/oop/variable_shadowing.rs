//! SV-OOP-004 (VariableShadowing): Detects variable shadowing where a derived
//! class property shadows a base class property, or a method argument / local
//! variable shadows a class member property.

use dv_model::Design;
use crate::{Severity, Sink, ancestors, find_property};

pub(crate) fn variable_shadowing(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        let chain = ancestors(d, cls);
        for p in &cls.properties {
            for &b in &chain {
                let base = d.class(b);
                let Some(shadow) = find_property(base, &p.name) else {
                    continue;
                };
                let f = out.add(
                    "SV-OOP-004",
                    "VariableShadowing",
                    Severity::Warning,
                    format!(
                        "Class property '{}' in derived class '{}' shadows property of the same name in base class '{}'. Accessing through a base class handle will reference the base member.",
                        p.name, cls.name, base.name
                    ),
                    p.loc,
                );
                f.related
                    .push((shadow.loc, format!("shadowed property in '{}'", base.name)));
                break;
            }
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.is_builtin {
                continue;
            }
            if sub.name != "new" {
                for a in &sub.arguments {
                    let Some(cp) = find_property(cls, &a.name) else {
                        continue;
                    };
                    let f = out.add(
                        "SV-OOP-004",
                        "VariableShadowing",
                        Severity::Warning,
                        format!(
                            "Method argument '{}' in method '{}' shadows class property '{}'. Use 'this.{}' or rename the argument to prevent silent assignment bugs.",
                            a.name, sub.name, cp.name, cp.name
                        ),
                        a.loc,
                    );
                    f.related.push((cp.loc, "class property".into()));
                }
            }
            for v in &sub.locals {
                let Some(cp) = find_property(cls, &v.name) else {
                    continue;
                };
                out.add(
                    "SV-OOP-004",
                    "VariableShadowing",
                    Severity::Warning,
                    format!(
                        "Local variable '{}' in method '{}' shadows class property '{}'. Rename the local variable to avoid confusion.",
                        v.name, sub.name, cp.name
                    ),
                    v.loc,
                );
            }
        }
    }
}
