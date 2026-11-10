//! SV-PKG-002 (WildcardImportOutsidePackage): Flags 'import pkg::*' at file
//! ($unit) scope, and inside files that are `include'd into a package.

use crate::support::defines_packages::{import_items, user_tok, Files};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use std::collections::HashSet;
use svlang::syntax::SyntaxKind::*;

pub(crate) fn wildcard_import_outside_package(_: &Design, csts: &[Cst], out: &mut Sink) {
    let mut seen = HashSet::new();
    for c in csts {
        let fs = Files::new(c);
        for n in c.nodes(ImportDecl) {
            let Some(kw) = c.keyword(n) else { continue };
            let Some(up) = c.parent(n) else { continue };
            if !user_tok(c, &fs, kw) {
                continue;
            }
            let o = c.tree.tokens[kw as usize].origin;
            if !seen.insert((fs.path(o.file), o.offset)) {
                continue;
            }
            for (p, item) in import_items(c, n) {
                if c.text_of(item) != "*" {
                    continue;
                }
                let pkg = c.text_of(p);
                let msg = match c.kind(up) {
                    SourceFile => format!(
                        "'import {pkg}::*' at file scope puts every name of {pkg} into $unit, where it is visible to every file compiled \
after this one. Move the import inside the module, interface or package that uses it."
                    ),
                    PackageDecl => {
                        let pf = c.first_tok(up).map(|t| &c.tree.tokens[t as usize]);
                        if pf.is_some_and(|t| t.origin.file == o.file && !t.from_macro) {
                            continue;
                        }
                        format!(
                            "'import {pkg}::*' sits in a file that is `include'd into a package, so it applies to every file included after \
it and the package's real dependencies are hidden. Import in the package file itself."
                        )
                    }
                    _ => continue,
                };
                out.add("SV-PKG-002", "WildcardImportOutsidePackage", Severity::Warning, msg, c.loc(kw));
            }
        }
    }
}
