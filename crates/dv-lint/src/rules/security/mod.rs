//! The security rules: the Rust twins of `cpp/src/rules/security/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::profile::*;
use crate::{Check, Rule, Severity};

mod plaintext_key_in_logs;
mod unchecked_fopen;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SEC-LOG-001", name: "PlaintextKeyInLogs", category: "Security", severity: Severity::Warning, profiles: SOUNDNESS | SECURITY, guidelines: &[], experimental: false, native: false, about: "Detects plaintext logging of sensitive cryptographic assets (keys, seeds, secrets) via $sformatf/$display. Logging secrets compromises hardware security and violates ISO/SAE 21434 regression standards.", check: Check::Mixed(plaintext_key_in_logs::ast_plaintext_key),
        },
        Rule { id: "SV-SYS-001", name: "UncheckedFopen", category: "Security", severity: Severity::Warning, profiles: SOUNDNESS | SECURITY, guidelines: &["MG-8.1"], experimental: false, native: false, about: "Flags unchecked $fopen return descriptors and discarded $fopen calls. File operations must check whether the descriptor is valid before performing file I/O.", check: Check::Syntax(unchecked_fopen::unchecked_fopen) },
    ]
}
