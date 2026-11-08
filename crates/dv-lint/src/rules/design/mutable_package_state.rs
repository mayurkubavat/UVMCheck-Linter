//! SV-DSN-008 (MutablePackageState): Flags non-const variables declared in
//! packages.

use crate::support::class_index::decl_names;
use crate::support::statements::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

pub(crate) fn mutable_package_state(c: &Cst, s: &mut Sink) {
    for p in c.nodes(PackageDecl) {
        let name = c.tree.child(p, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default();
        if name == "std" || name == "uvm_pkg" {
            continue;
        }
        if c.first_tok(p).is_some_and(|t| {
            let f = file_of(c, t);
            f.contains("uvm_mock.svh") || f.contains("uvm_pkg.sv")
        }) {
            continue;
        }
        for item in c.tree.children(p).filter(|&x| c.kind(x) == DataDecl) {
            if c.toks(item).any(|t| matches!(c.text_of(t), "const" | "localparam" | "parameter")) {
                continue;
            }
            // Nets and instances are not variables.
            if is_net_or_instance(c, item) {
                continue;
            }
            for (var, tok) in decl_names(c, item) {
                s.add(
                    "SV-DSN-008",
                    "MutablePackageState",
                    Severity::Warning,
                    format!(
                        "Package variable '{name}::{var}' is global mutable state shared by every env instance and test in the run. \
                         Move it into a config object or component, or make it const."
                    ),
                    c.loc(tok),
                );
            }
        }
    }
}

fn is_net_or_instance(c: &Cst, d: NodeId) -> bool {
    let nets = [
        "wire",
        "tri",
        "wand",
        "wor",
        "tri0",
        "tri1",
        "supply0",
        "supply1",
        "triand",
        "trior",
        "trireg",
        "uwire",
        "interconnect",
    ];
    let first = c.first_tok(d).map(|t| c.text_of(t)).unwrap_or("");
    nets.contains(&first)
        || c.tree.child(d, DataType).and_then(|t| c.first_tok(t)).is_some_and(|t| nets.contains(&c.text_of(t)))
}
