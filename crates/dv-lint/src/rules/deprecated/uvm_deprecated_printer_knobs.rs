//! UVM-DEP-003 (UvmDeprecatedPrinterKnobs): Disallow direct access to
//! 'printer.knobs' (IEEE 1800.2 § 5.5.2).

use crate::support::copies::copies;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::HashMap;
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

/// The declared type of variable `name` as seen from `at`: the closest
/// declaration in an enclosing scope (data declarations and ports).
fn declared_type(c: &Cst, at: NodeId, name: &str, decls: &HashMap<String, Vec<(NodeId, String)>>) -> Option<String> {
    let cands = decls.get(name)?;
    let mut up = c.parent(at);
    while let Some(p) = up {
        // A declaration whose scope (parent of its declaration) is `p`.
        for (scope, ty) in cands {
            if *scope == p {
                return Some(ty.clone());
            }
        }
        up = c.parent(p);
    }
    None
}

pub(crate) fn deprecated_printer_knobs(c: &Cst, s: &mut Sink) {
    // Declared names by the scope that holds the declaration.
    let mut decls: HashMap<String, Vec<(NodeId, String)>> = HashMap::new();
    for d in c.nodes(DataDecl).into_iter().chain(c.nodes(Port)) {
        let Some(ty) = c.tree.child(d, DataType) else { continue };
        let ty = c.tree.bare_text(ty);
        let mut scope = c.parent(d);
        // Statements in a block or body: the block is the scope.
        while let Some(p) = scope {
            if matches!(c.kind(p), PortList | Header) {
                scope = c.parent(p);
                continue;
            }
            break;
        }
        let Some(scope) = scope else { continue };
        for dd in c.tree.children(d).filter(|&x| c.kind(x) == Declarator) {
            let Some(nm) = c.tree.child(dd, Name) else { continue };
            decls.entry(c.tree.bare_text(nm)).or_default().push((scope, ty.clone()));
        }
    }
    for n in c.nodes(MemberExpr) {
        if c.last_text(n) != "knobs" {
            continue;
        }
        let Some(recv) = c.tree.children(n).next() else { continue };
        if c.kind(recv) != NameExpr {
            continue;
        }
        let Some(ty) = declared_type(c, n, &c.tree.bare_text(recv), &decls) else { continue };
        if !ty.contains("printer") {
            continue;
        }
        let Some(first) = c.first_tok(n) else { continue };
        for _ in 0..copies(c, n) {
            s.add(
                "UVM-DEP-003",
                "UvmDeprecatedPrinterKnobs",
                Severity::Warning,
                "Direct access to internal 'printer.knobs' is deprecated in IEEE 1800.2 § 5.5.2. \
                 Use official policy accessor methods on uvm_printer (e.g. 'printer.set_max_depth()', 'printer.set_radix()')."
                    .into(),
                c.loc(first),
            );
        }
    }
}
