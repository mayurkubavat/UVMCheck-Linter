//! RAL-COL-001 (RalAddressCollision): Detects duplicate/overlapping address
//! offsets mapped in uvm_reg_block address maps.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::uvm_syntax::{name_loc, reg_block_builds, routine_text, strip_line_comments, trim_ws};

pub(crate) fn ral_address_collision(c: &Cst, s: &mut Sink) {
    for (cls, f) in reg_block_builds(c) {
        let body = strip_line_comments(&routine_text(c, f));
        let mut by_offset: std::collections::HashMap<String, String> = Default::default();
        let mut pos = 0;
        while let Some(at) = body[pos..].find(".add_reg").map(|i| i + pos) {
            pos = at;
            if let Some(open) = body[at..].find('(').map(|i| i + at) {
                if let Some(c1) = body[open..].find(',').map(|i| i + open) {
                    let c2 = body[c1 + 1..].find(',').map(|i| i + c1 + 1);
                    let close = body[c1 + 1..].find(')').map(|i| i + c1 + 1);
                    let end = match (c2, close) {
                        (Some(a), Some(b)) if a < b => Some(a),
                        (Some(a), None) => Some(a),
                        (_, b) => b,
                    };
                    if let Some(end) = end {
                        let reg = trim_ws(&body[open + 1..c1]);
                        let off = trim_ws(&body[c1 + 1..end]);
                        if !reg.is_empty() && !off.is_empty() {
                            if let Some(prev) = by_offset.get(off) {
                                let Some(loc) = name_loc(c, f) else { break };
                                s.add(
                                    "RAL-COL-001",
                                    "RalAddressCollision",
                                    Severity::Error,
                                    format!(
                                        "Register address collision in register block '{cls}': register '{reg}' is mapped to offset {off}, which is already occupied by register '{prev}'. Address collisions cause silent register overwrites and aliasing bugs."
                                    ),
                                    loc,
                                );
                            } else {
                                by_offset.insert(off.to_string(), reg.to_string());
                            }
                        }
                    }
                }
            }
            pos += 8;
        }
    }
}
