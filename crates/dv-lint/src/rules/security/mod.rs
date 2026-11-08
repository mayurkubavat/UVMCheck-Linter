//! The security rules: the Rust twins of `cpp/src/rules/security/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

mod plaintext_key_in_logs;
mod unchecked_fopen;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule {
            id: "SEC-LOG-001",
            name: "PlaintextKeyInLogs",
            check: Check::Mixed(plaintext_key_in_logs::ast_plaintext_key),
        },
        Rule { id: "SV-SYS-001", name: "UncheckedFopen", check: Check::Syntax(unchecked_fopen::unchecked_fopen) },
    ]
}
