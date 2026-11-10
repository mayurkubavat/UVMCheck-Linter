//! The compiler rules: the Rust twins of `cpp/src/rules/compiler/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::profile::*;
use crate::{Check, Rule, Severity};

mod compiler_error;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![Rule { id: "SV-CMP-001", name: "CompilerError", category: "Correctness", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Reports the SystemVerilog compiler's errors (syntax, elaboration and type errors) as findings, so they appear in every report and in the editor next to the lint findings.", check: Check::Syntax(compiler_error::compiler_errors) }]
}
