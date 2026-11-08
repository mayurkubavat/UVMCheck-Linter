//! SV-OOP-014 (SuperNewNotFirstStatement): Derived class constructors calling
//! super.new(...) must invoke it as the very first procedural statement.

use dv_model::Design;
use crate::{Severity, Sink, find_method};

pub(crate) fn super_new_not_first_statement(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized || cls.base.is_none() {
            continue;
        }
        let Some(ctor) = find_method(cls, "new") else {
            continue;
        };
        if ctor.is_builtin {
            continue;
        }
        let Some((idx, loc)) = ctor.super_new else {
            continue;
        };
        if idx == 0 {
            continue;
        }
        out.add(
            "SV-OOP-014",
            "SuperNewNotFirstStatement",
            Severity::Warning,
            format!(
                "Constructor 'new' in derived class '{}' invokes 'super.new(...)' after other procedural statements. In SystemVerilog OOP \
(IEEE 1800 § 8.17), 'super.new(...)' must be the \
very first procedural statement in a derived class constructor to ensure base class properties \
and infrastructure are fully initialized before derived logic executes.",
                cls.name
            ),
            loc,
        );
    }
}
