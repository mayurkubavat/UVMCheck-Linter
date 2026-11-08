//! RAL-MAP-002 (RalMapAddressIntervalOverlap): Detects 1D address interval
//! overlaps and register aliasing collisions within the same uvm_reg_map in a
//! uvm_reg_block, preventing silent register corruption in hardware.

use std::collections::{HashMap, HashSet};
use dv_model::{Design, Loc};
use sv_syntax::{NodeId, SyntaxKind};
use crate::support::uvm_syntax::{
    decl_name, trim_ws,
};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::uvm_classes::{Ix, args, call_name, calls_in, derived, int_value, unparen};

/// Names a `X::` scope may start with: classes, packages and typedefs.
fn scope_names(csts: &[Cst]) -> HashSet<String> {
    let mut out: HashSet<String> = ["std", "$unit", "local", "super", "this", "$root"].iter().map(|s| s.to_string()).collect();
    for c in csts {
        for k in [PackageDecl, ClassDecl, ForwardTypedef] {
            for n in c.nodes(k) {
                out.insert(decl_name(c, n));
            }
        }
        for n in c.nodes(TypedefDecl) {
            for x in crate::support::uvm_syntax::subtree(c, n) {
                if c.kind(x) == Name {
                    out.insert(c.tree.bare_text(x));
                }
            }
        }
    }
    out
}

/// Slang drops a body with an unresolved `X::` from AST visits.
fn bad_body(c: &Cst, n: NodeId, known: &HashSet<String>) -> bool {
    crate::support::uvm_syntax::subtree(c, n).into_iter().any(|x| {
        c.kind(x) == ScopedExpr && c.tree.token_range(x).first().is_some_and(|&t| !known.contains(c.text_of(t)))
    })
}

pub(crate) fn ral_map_overlap(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    let known = scope_names(csts);
    for cls in ix.model_classes() {
        if !(cls.name.contains("reg_block") || derived(d, cls.id, "uvm_reg_block")) {
            continue;
        }
        let width = |name: &str, ty: &str| -> u64 {
            if ty.contains("64") || name.contains("64") {
                8
            } else if ty.contains("16") {
                2
            } else if ty.contains('8') {
                1
            } else {
                4
            }
        };
        let sizes: HashMap<&str, u64> = cls.properties.iter().map(|p| (p.name.as_str(), width(&p.name, &p.ty))).collect();
        let mut entries: Vec<(String, u64, u64, Loc)> = Vec::new();
        for at in ix.own_routines(cls.id) {
            let c = ix.cst(at);
            if bad_body(c, at.node, &known) {
                continue;
            }
            for n in calls_in(c, at.node) {
                if call_name(c, n) != "add_reg" {
                    continue;
                }
                let a = args(c, n);
                if a.len() < 2 {
                    continue;
                }
                let (Some(r), Some(o)) = (a[0], a[1]) else { continue };
                let r = unparen(c, r);
                let reg = if c.kind(r) == NameExpr {
                    c.tree.bare_text(r)
                } else {
                    let t = c.text(r);
                    let t = t.rfind('.').map_or(t.as_str(), |i| &t[i + 1..]).to_string();
                    trim_ws(&t).to_string()
                };
                let Some(off) = offset_value(&c.text(o)) else { continue };
                let size = sizes.get(reg.as_str()).copied().unwrap_or_else(|| {
                    if reg.contains("64") {
                        8
                    } else if reg.contains("16") {
                        2
                    } else if reg.contains('8') {
                        1
                    } else {
                        4
                    }
                });
                let Some(t) = c.first_tok(n) else { continue };
                entries.push((reg, off, size, c.loc(t)));
            }
        }
        for i in 0..entries.len() {
            for j in i + 1..entries.len() {
                let (r1, s1, z1, _) = &entries[i];
                let (r2, s2, z2, l2) = &entries[j];
                let (e1, e2) = (s1.wrapping_add(*z1).wrapping_sub(1), s2.wrapping_add(*z2).wrapping_sub(1));
                if s1 <= &e2 && s2 <= &e1 {
                    s.add(
                        "RAL-MAP-002",
                        "RalMapAddressIntervalOverlap",
                        Severity::Error,
                        format!(
                            "Address interval overlap detected in uvm_reg_map: register '{r2}' ['h{s2}..'h{e2}] collides with previously mapped register '{r1}' ['h{s1}..'h{e1}]. Register address aliasing will cause hardware overwrite corruption."
                        ),
                        *l2,
                    );
                }
            }
        }
    }
}

/// The add_reg offset as the C++ reads it: an integer literal, else
/// `std::stoull` on the text (after `'h`, or with base 0).
fn offset_value(text: &str) -> Option<u64> {
    let t = trim_ws(text);
    if let Some(v) = int_value(t) {
        if !t.contains(['x', 'X', 'z', 'Z', '?']) || !t.contains('\'') {
            return u64::try_from(v).ok();
        }
    }
    if let Some(h) = text.find("'h") {
        return stoull(&text[h + 2..], 16);
    }
    stoull(text, 0)
}

/// `std::stoull`: leading whitespace, then the longest valid prefix.
fn stoull(s: &str, base: u32) -> Option<u64> {
    let s = s.trim_start();
    let (radix, digits) = if base == 0 {
        if let Some(r) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            (16, r)
        } else if s.starts_with('0') && s.len() > 1 {
            (8, &s[1..])
        } else {
            (10, s)
        }
    } else {
        let r = if base == 16 { s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s) } else { s };
        (base, r)
    };
    let end = digits.find(|c: char| !c.is_digit(radix)).unwrap_or(digits.len());
    if end == 0 {
        // "0" alone in base 0 is octal zero.
        return (base == 0 && s.starts_with('0')).then_some(0);
    }
    u64::from_str_radix(&digits[..end], radix).ok()
}
