//! SV-PRF-006 (StringKeyAssociativeArray): Detects associative arrays indexed
//! by 'string'.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

/// The first unpacked dimension is `[string]`. A typedef'd associative
/// type is an alias in slang, which the rule does not look through.
fn string_keyed(c: &Cst, dcl: NodeId) -> bool {
    c.tree.children(dcl).find(|&d| c.kind(d) == Dim).is_some_and(|d| {
        c.tree.children(d).count() == 1
            && c.tree.children(d).next().is_some_and(|k| c.kind(k) == DataType && c.tree.bare_text(k) == "string")
    })
}

pub(crate) fn string_key_assoc(c: &Cst, s: &mut Sink) {
    let decls = c.nodes(DataDecl).into_iter().chain(c.nodes(Port));
    for dd in decls {
        // Struct members are fields, not variables; module ports are nets.
        if c.kind(dd) == DataDecl
            && std::iter::successors(c.parent(dd), |&a| c.parent(a)).any(|a| c.kind(a) == DataType)
        {
            continue;
        }
        if c.kind(dd) == Port
            && !c.parent(dd).and_then(|l| c.parent(l)).is_some_and(|r| matches!(c.kind(r), FunctionDecl | TaskDecl))
        {
            continue;
        }
        for dcl in c.tree.children(dd).filter(|&d| c.kind(d) == Declarator) {
            if !string_keyed(c, dcl) {
                continue;
            }
            let Some(nm) = c.tree.child(dcl, Name) else { continue };
            let Some((tok, _)) = c.tree.token_bounds(nm) else { continue };
            let name = c.tree.bare_text(nm);
            for _ in 0..symbol_times(c, dd) {
                s.add(
                    "SV-PRF-006",
                    "StringKeyAssociativeArray",
                    Severity::Warning,
                    format!(
                        "Associative array '{name}' is indexed by 'string'. Associative string lookups incur dynamic hashing and heap memory allocations on every element access, severely degrading simulation performance in hot transaction paths. Use an integral type, enum, or integer ID instead."
                    ),
                    c.loc(tok),
                );
            }
        }
    }
}

/// How often a symbol visitor from the root reaches a declaration: once per
/// instance of its module or specialization of its class.
fn symbol_times(c: &Cst, n: NodeId) -> usize {
    crate::support::literals::units(c).times(c, n)
}
