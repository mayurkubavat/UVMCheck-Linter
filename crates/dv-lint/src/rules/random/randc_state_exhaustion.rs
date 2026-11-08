//! SV-RND-017 (RandcStateExhaustion): Flags 'randc' cyclic variables declared
//! with bit-width <= 2 bits (1 or 2 bits).

use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::constraints::{randc_vars};

pub(crate) fn randc_narrow(c: &Cst, s: &mut Sink) {
    for (tok, name, w) in randc_vars(c) {
        if !(1..=2).contains(&w) {
            continue;
        }
        let states = 1u64 << w;
        let plural = if w > 1 { "s" } else { "" };
        s.add(
            "SV-RND-017",
            "RandcStateExhaustion",
            Severity::Warning,
            format!(
                "Cyclic random variable '{name}' is declared as 'randc' with a width of {w} bit{plural} (only {states} \
                 states). In IEEE 1800 § 18.4.1, 'randc' forces the constraint solver to maintain a non-repeating \
                 permutation table. For variables <= 2 bits, cyclic permutation history resets every {states} iterations, \
                 incurring solver tracking overhead with no statistical permutation benefit. Use standard 'rand' instead."
            ),
            c.loc(tok),
        );
    }
}
