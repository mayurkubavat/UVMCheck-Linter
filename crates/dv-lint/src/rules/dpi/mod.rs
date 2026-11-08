//! The dpi rules: the Rust twins of `cpp/src/rules/dpi/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

mod dpi_type_width_alignment;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![Rule {
        id: "DPI-TYP-001",
        name: "DpiTypeWidthAlignment",
        check: Check::Syntax(dpi_type_width_alignment::dpi_type_width),
    }]
}
