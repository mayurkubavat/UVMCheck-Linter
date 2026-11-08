//! The compiler rules: the Rust twins of `cpp/src/rules/compiler/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

mod compiler_error;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-CMP-001", name: "CompilerError", check: Check::Syntax(compiler_error::compiler_errors) },
    ]
}
