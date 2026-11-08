//! Every rule, one folder per category as under `cpp/src/rules/`, one file per
//! rule named after the rule (`SV-OOP-003` MissingSuperNew is
//! `oop/missing_super_new.rs`). Helpers several rules share live outside
//! `rules/`, in the support modules of the crate root.

use crate::Rule;

mod analysis;
mod compiler;
mod concurrency;
mod correctness;
mod coverage;
mod deadcode;
mod deprecated;
mod design;
mod dpi;
mod errors;
mod methodology;
mod metrics;
mod oop;
mod performance;
mod portability;
mod ral;
mod random;
mod readability;
mod security;
mod smells;
pub(crate) mod style;
mod sva;
mod types;
pub(crate) mod uvm;

/// Every rule of every category, unsorted.
pub(crate) fn all() -> Vec<Rule> {
    let mut v = Vec::new();
    v.extend(analysis::rules());
    v.extend(compiler::rules());
    v.extend(concurrency::rules());
    v.extend(correctness::rules());
    v.extend(coverage::rules());
    v.extend(deadcode::rules());
    v.extend(deprecated::rules());
    v.extend(design::rules());
    v.extend(dpi::rules());
    v.extend(errors::rules());
    v.extend(methodology::rules());
    v.extend(metrics::rules());
    v.extend(oop::rules());
    v.extend(performance::rules());
    v.extend(portability::rules());
    v.extend(ral::rules());
    v.extend(random::rules());
    v.extend(readability::rules());
    v.extend(security::rules());
    v.extend(smells::rules());
    v.extend(style::rules());
    v.extend(sva::rules());
    v.extend(types::rules());
    v.extend(uvm::rules());
    v
}

/// See [`crate::naming_pattern_supported`].
pub(crate) fn naming_pattern_supported(target: &str, pattern: &str) -> bool {
    style::configurable_naming_engine::target_supported(target, pattern)
}
