//! The ral rules: the Rust twins of `cpp/src/rules/ral/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::profile::*;
use crate::{Check, Rule, Severity};

mod direct_mirror_set_without_write;
mod missing_backdoor_access_path_definition;
mod ral_address_collision;
mod ral_orphan_register;
mod reg_block_never_locked;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "RAL-ACC-001", name: "DirectMirrorSetWithoutWrite", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Flags direct reg.set(...) calls that update only software mirrors without issuing physical DUT write transactions.", check: Check::Mixed(direct_mirror_set_without_write::direct_mirror_set),
        },
        Rule { id: "RAL-ACC-002", name: "MissingBackdoorAccessPathDefinition", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Flags RAL backdoor path configuration routines called with an empty string or null path.", check: Check::Syntax(missing_backdoor_access_path_definition::backdoor_path),
        },
        Rule { id: "RAL-BLK-001", name: "RegBlockNeverLocked", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Flags a top-level register block (held by an env or test, not by another block) on which lock_model() is never called. Until the model is locked, address maps are not built, so frontdoor accesses and get_reg_by_offset() fail.", check: Check::Mixed(reg_block_never_locked::reg_block_never_locked),
        },
        Rule { id: "RAL-COL-001", name: "RalAddressCollision", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Detects duplicate/overlapping address offsets mapped in uvm_reg_block address maps. Address collisions cause silent register overwriting and hardware aliasing bugs.", check: Check::Syntax(ral_address_collision::ral_address_collision),
        },
        Rule { id: "RAL-MAP-001", name: "RalOrphanRegister", category: "UVM", severity: Severity::Error, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: false, about: "Detects instantiated uvm_reg instances inside uvm_reg_block that are never mapped via default_map.add_reg(). Orphan registers cause frontdoor access failures and verify phantom memory.", check: Check::Syntax(ral_orphan_register::ral_orphan_register),
        },
    ]
}
