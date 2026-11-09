//! SV-OOP-004 (VariableShadowing): Detects variable shadowing where a derived
//! class property shadows a base class property, or a method argument / local
//! variable shadows a class member property.
//!
//! A class member is its own property or one it inherits and can see (not a
//! base class's `local` one): a local `req` in a `uvm_sequence` body hides the
//! `req` the sequence inherits. A static method sees no instance property,
//! so nothing it declares shadows one. The C++ rule looks up the same way.

use crate::{ancestors, find_property, Severity, Sink};
use dv_model::{Class, Design, Property, Visibility};

/// The member `name` names in `cls`: its own property, else the nearest
/// visible one of a class it extends, with the class that declares it.
fn member<'d>(d: &'d Design, cls: &'d Class, name: &str) -> Option<(&'d Property, &'d Class)> {
    if let Some(p) = find_property(cls, name) {
        return Some((p, cls));
    }
    for b in ancestors(d, cls) {
        let base = d.class(b);
        if let Some(p) = find_property(base, name) {
            return (p.visibility != Visibility::Local).then_some((p, base));
        }
    }
    None
}

/// `'x'` or `'x' inherited from 'base'`.
fn which(p: &Property, owner: &Class, cls: &Class) -> String {
    if owner.id == cls.id {
        format!("'{}'", p.name)
    } else {
        format!("'{}' inherited from '{}'", p.name, owner.name)
    }
}

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
                f.related.push((shadow.loc, format!("shadowed property in '{}'", base.name)));
                break;
            }
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.is_builtin {
                continue;
            }
            if sub.name != "new" {
                for a in &sub.arguments {
                    let Some((cp, owner)) = member(d, cls, &a.name).filter(|m| !sub.is_static || m.0.is_static) else {
                        continue;
                    };
                    let f = out.add(
                        "SV-OOP-004",
                        "VariableShadowing",
                        Severity::Warning,
                        format!(
                            "Method argument '{}' in method '{}' shadows class property {}. Use 'this.{}' or rename the argument to prevent silent assignment bugs.",
                            a.name,
                            sub.name,
                            which(cp, owner, cls),
                            cp.name
                        ),
                        a.loc,
                    );
                    f.related.push((cp.loc, "class property".into()));
                }
            }
            for v in &sub.locals {
                let Some((cp, owner)) = member(d, cls, &v.name).filter(|m| !sub.is_static || m.0.is_static) else {
                    continue;
                };
                out.add(
                    "SV-OOP-004",
                    "VariableShadowing",
                    Severity::Warning,
                    format!(
                        "Local variable '{}' in method '{}' shadows class property {}. Rename the local variable to avoid confusion.",
                        v.name,
                        sub.name,
                        which(cp, owner, cls)
                    ),
                    v.loc,
                );
            }
        }
    }
}
