//! SV-PKG-001 (FileClassNameMismatch): Flags `include'd class files that define
//! more than one top-level class, or whose one class is not named after the
//! file (my_drv.svh holding my_driver).

use crate::support::defines_packages::{base_name, Files};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::{Design, Loc};
use std::collections::{BTreeMap, HashSet};
use sv_syntax::SyntaxKind::*;

#[derive(Default)]
struct ClassFile {
    classes: Vec<(String, Loc)>,
    has_other_units: bool,
}

pub(crate) fn file_class_name_mismatch(_: &Design, csts: &[Cst], out: &mut Sink) {
    let mut files: BTreeMap<String, ClassFile> = BTreeMap::new();
    let mut seen = HashSet::new();
    for c in csts {
        let fs = Files::new(c);
        // Only `include'd user files: the class files of a package.
        let file_of = |t: u32| -> Option<String> {
            let tok = &c.tree.tokens[t as usize];
            (!tok.from_macro && !c.tree.mains.contains(&tok.origin.file) && fs.user(tok.origin.file))
                .then(|| fs.path(tok.origin.file))
        };
        for n in c.nodes_of(&[ClassDecl, ModuleDecl, InterfaceDecl, ProgramDecl, PackageDecl]) {
            let Some(first) = c.first_tok(n) else { continue };
            let Some(path) = file_of(first) else { continue };
            if c.kind(n) != ClassDecl {
                files.entry(path).or_default().has_other_units = true;
                continue;
            }
            if !c.parent(n).is_some_and(|p| matches!(c.kind(p), SourceFile | PackageDecl)) {
                continue;
            }
            let Some(name) = c.tree.child(n, Name) else { continue };
            let Some((_, nt)) = c.tree.token_bounds(name) else { continue };
            if !seen.insert((path.clone(), c.tree.tokens[first as usize].origin.offset)) {
                continue;
            }
            files.entry(path).or_default().classes.push((c.tree.bare_text(name), c.loc(nt)));
        }
    }
    for (path, f) in &files {
        if f.classes.is_empty() || f.has_other_units {
            continue;
        }
        let file = base_name(path);
        let stem = file.split('.').next().unwrap_or(file);
        if f.classes.len() == 1 {
            let (name, loc) = &f.classes[0];
            if name == stem {
                continue;
            }
            out.add(
                "SV-PKG-001",
                "FileClassNameMismatch",
                Severity::Warning,
                format!(
                    "'{file}' holds class '{name}'. Name the file after the class ({name}.svh) so it can be found from the class name."
                ),
                *loc,
            );
            continue;
        }
        // Keep the class the file is named after (or the first one).
        let keep = f.classes.iter().rposition(|(n, _)| n == stem).unwrap_or(0);
        for (i, (name, loc)) in f.classes.iter().enumerate() {
            if i == keep {
                continue;
            }
            out.add(
                "SV-PKG-001",
                "FileClassNameMismatch",
                Severity::Warning,
                format!(
                    "'{file}' defines {} classes. Move '{name}' to its own file, {name}.svh, so that each class can be found and reused \
on its own.",
                    f.classes.len()
                ),
                *loc,
            );
        }
    }
}
