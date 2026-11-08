//! SV-DEP-003 (LegacyAlwaysBlock): Disallow untyped 'always' blocks.

use crate::{Severity, Sink};
use crate::syntax::{Cst, procedural_blocks};

pub(crate) fn legacy_always(c: &Cst, s: &mut Sink) {
    for (_, kw) in procedural_blocks(c) {
        if c.text_of(kw) == "always" {
            s.add(
                "SV-DEP-003",
                "LegacyAlwaysBlock",
                Severity::Warning,
                "Legacy untyped 'always' block is superseded in IEEE 1800 § 9.2.2. \
                 Untyped always blocks permit unintended latch inference and incomplete sensitivity lists. \
                 Use intent-specific procedural blocks: 'always_comb' for combinational logic, \
                 'always_ff @(posedge clk ...)' for sequential logic, or 'always_latch' for transparent latches."
                    .into(),
                c.loc(kw),
            );
        }
    }
}
