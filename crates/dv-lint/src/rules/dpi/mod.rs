//! The dpi rules: the Rust twins of `cpp/src/rules/dpi/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::profile::*;
use crate::{Check, Rule, Severity};

mod dpi_type_width_alignment;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![Rule { id: "DPI-TYP-001", name: "DpiTypeWidthAlignment", category: "DPI", severity: Severity::Warning, profiles: SOUNDNESS | SECURITY, guidelines: &[], experimental: false, native: false, about: "Disallow 4-state types (integer, reg) in DPI-C declarations. 4-state types require complex svLogicVecVal structs in C; use 2-state portable types (int, longint, byte) for C99 ABI compatibility.", check: Check::Syntax(dpi_type_width_alignment::dpi_type_width),
    }]
}
