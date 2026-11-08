//! The ral rules: the Rust twins of `cpp/src/rules/ral/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

mod direct_mirror_set_without_write;
mod missing_backdoor_access_path_definition;
mod ral_address_collision;
mod ral_orphan_register;
mod reg_block_never_locked;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "RAL-ACC-001", name: "DirectMirrorSetWithoutWrite", check: Check::Mixed(direct_mirror_set_without_write::direct_mirror_set) },
        Rule { id: "RAL-ACC-002", name: "MissingBackdoorAccessPathDefinition", check: Check::Syntax(missing_backdoor_access_path_definition::backdoor_path) },
        Rule { id: "RAL-BLK-001", name: "RegBlockNeverLocked", check: Check::Mixed(reg_block_never_locked::reg_block_never_locked) },
        Rule { id: "RAL-COL-001", name: "RalAddressCollision", check: Check::Syntax(ral_address_collision::ral_address_collision) },
        Rule { id: "RAL-MAP-001", name: "RalOrphanRegister", check: Check::Syntax(ral_orphan_register::ral_orphan_register) },
    ]
}
