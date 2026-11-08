//! UVM-ARC-003 (FileIncludedInMultiplePackages): Flags a file whose class
//! definitions are `include'd into more than one package.

use crate::support::statements::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::BTreeMap;
use sv_syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn file_in_many_packages(c: &Cst, s: &mut Sink) {
    // file -> [(package name, package name token, class name)]
    let mut by_file: BTreeMap<String, Vec<(String, u32, String)>> = BTreeMap::new();
    for p in c.nodes(PackageDecl) {
        let Some(nm) = c.tree.child(p, Name) else { continue };
        let Some(ptok) = c.first_tok(nm) else { continue };
        let pname = c.text_of(ptok).to_string();
        let pfile = file_of(c, ptok);
        if pname == "std" || pname == "uvm_pkg" || pfile.contains("uvm_mock.svh") || pfile.contains("uvm_pkg.sv") {
            continue;
        }
        for k in c.tree.children(p).filter(|&x| c.kind(x) == ClassDecl) {
            let Some(knm) = c.tree.child(k, Name) else { continue };
            let Some(ktok) = c.first_tok(knm) else { continue };
            let kfile = file_of(c, ktok);
            if kfile.is_empty() || kfile == pfile {
                continue;
            }
            let sites = by_file.entry(kfile).or_default();
            if !sites.iter().any(|(n, _, _)| *n == pname) {
                sites.push((pname.clone(), ptok, c.text_of(ktok).to_string()));
            }
        }
    }
    for sites in by_file.values() {
        if sites.len() < 2 {
            continue;
        }
        let pkgs = sites.iter().map(|(n, _, _)| n.as_str()).collect::<Vec<_>>().join(", ");
        for (_, ptok, cls) in &sites[1..] {
            s.add(
                "UVM-ARC-003",
                "FileIncludedInMultiplePackages",
                Severity::Error,
                format!(
                    "'{cls}' is compiled into packages {pkgs} because its file is `include'd into each. That creates distinct types with the \
                     same name, so $cast, config_db and TLM connections between them fail at run time. \
                     Include it in one package and import that package elsewhere."
                ),
                c.loc(*ptok),
            );
        }
    }
}
