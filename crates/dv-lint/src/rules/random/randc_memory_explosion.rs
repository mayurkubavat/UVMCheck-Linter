//! SV-RND-006 (RandcMemoryExplosion): Disallow wide vector declarations with
//! 'randc' (> 16 bits).

use crate::support::constraints::randc_vars;
use crate::syntax::Cst;
use crate::{Severity, Sink};

pub(crate) fn randc_wide(c: &Cst, s: &mut Sink) {
    for (tok, name, w) in randc_vars(c) {
        if w <= 16 {
            continue;
        }
        s.add(
            "SV-RND-006",
            "RandcMemoryExplosion",
            Severity::Error,
            format!(
                "Cyclic random variable '{name}' is declared as 'randc' with a width of {w} bits (2^{w} states). In IEEE \
                 1800 § 18.4.1, randc variables require the constraint solver to maintain a non-repeating permutation \
                 table across the entire state space. Vectors wider than 16 bits cause massive simulator heap exhaustion \
                 and extreme runtime deadlocks during solver initialization. Restrict 'randc' to variables <= 16 bits, or \
                 use 'rand'."
            ),
            c.loc(tok),
        );
    }
}
