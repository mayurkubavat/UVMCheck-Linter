//! SV-RND-002 (GlobalRngInClass): Calling $urandom or $random inside class
//! components or sequences corrupts global thread random stability.

use crate::{Severity, Sink};
use dv_model::Design;

pub(crate) fn global_rng_in_class(d: &Design, out: &mut Sink) {
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern || sub.is_builtin {
                continue;
            }
            for c in &sub.calls {
                if c.name != "$urandom" && c.name != "$urandom_range" && c.name != "$random" {
                    continue;
                }
                out.add(
                    "SV-RND-002",
                    "GlobalRngInClass",
                    Severity::Warning,
                    format!(
                        "Invoking global pseudo-random system function '{}' inside class '{}'. Global RNG calls consume the process-level PRNG state instead of the object's hierarchical random stream, \
destroying simulation seed repeatability and testbench reproducibility. Use 'std::randomize()' or class-scoped RNG.",
                        c.name, cls.name
                    ),
                    c.loc,
                );
            }
        }
    }
}
