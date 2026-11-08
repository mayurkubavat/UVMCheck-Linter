//! SV-RND-018 (RandomizeInConstructor): Disallows calling randomize() inside
//! class constructors (new()).

use dv_model::Design;
use crate::{Severity, Sink};

pub(crate) fn randomize_in_constructor(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.name != "new" || sub.is_builtin {
                continue;
            }
            for c in &sub.calls {
                if c.name != "randomize" {
                    continue;
                }
                out.add(
                    "SV-RND-018",
                    "RandomizeInConstructor",
                    Severity::Warning,
                    format!(
                        "Calling 'randomize()' inside class constructor 'new()' of class '{}'. In SystemVerilog OOP, calling randomize() during object construction executes before derived class members, \
constraints, and factory overrides are fully constructed and initialized. \
Stimulus randomization should always occur explicitly on fully constructed instances (e.g. 'if (!item.randomize()) ...').",
                        cls.name
                    ),
                    c.loc,
                );
            }
        }
    }
}
