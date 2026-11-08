//! RAL-MAP-001 (RalOrphanRegister): Detects instantiated uvm_reg instances
//! inside uvm_reg_block that are never mapped via default_map.add_reg().

use crate::support::uvm_syntax::{name_loc, reg_block_builds, routine_text, strip_line_comments, trim_ws};
use crate::syntax::Cst;
use crate::{Severity, Sink};

pub(crate) fn ral_orphan_register(c: &Cst, s: &mut Sink) {
    for (cls, f) in reg_block_builds(c) {
        let body = strip_line_comments(&routine_text(c, f));
        let mut mapped = std::collections::HashSet::new();
        let mut pos = 0;
        while let Some(at) = body[pos..].find(".add_reg").map(|i| i + pos) {
            if let Some(open) = body[at..].find('(').map(|i| i + at) {
                if let Some(comma) = body[open..].find(',').map(|i| i + open) {
                    let reg = trim_ws(&body[open + 1..comma]);
                    if !reg.is_empty() {
                        mapped.insert(reg.to_string());
                    }
                }
            }
            pos = at + 8;
        }
        let mut made = std::collections::BTreeSet::new();
        let mut pos = 0;
        while let Some(at) = body[pos..].find("::type_id::create").map(|i| i + pos) {
            if let Some(eq) = body[..=at].rfind('=') {
                let start = body[..=eq].rfind('\n').map_or(0, |i| i + 1);
                let lhs = trim_ws(&body[start..eq]);
                if !lhs.is_empty() && !lhs.contains(' ') {
                    made.insert(lhs.to_string());
                }
            }
            pos = at + 18;
            if pos > body.len() {
                break;
            }
        }
        for reg in made {
            if mapped.contains(&reg) {
                continue;
            }
            let Some(loc) = name_loc(c, f) else { continue };
            s.add(
                "RAL-MAP-001",
                "RalOrphanRegister",
                Severity::Error,
                format!(
                    "Orphan register '{reg}' in register block '{cls}': instantiated in build() but never mapped via 'default_map.add_reg({reg}, ...)'. Orphan registers lack an address mapping, causing frontdoor write/read operations to fail."
                ),
                loc,
            );
        }
    }
}
