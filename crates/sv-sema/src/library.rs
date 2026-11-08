//! Library search, as VCS and Xcelium do it: `-v FILE` names a library
//! file and `-y DIR` a library directory (with `+libext+.v+.sv` the
//! extensions tried). Neither is a source: after the sources are parsed,
//! every module, interface, program or primitive that is instantiated but
//! not defined is looked up, first in the `-v` files (a file is read when
//! it defines a missing name), then as `DIR/<name><ext>` in each `-y`
//! directory in order (with no `+libext+`, `DIR/<name>` only). A file read
//! may instantiate more missing names, so the search repeats until nothing
//! new is found. Names that stay missing are left as they were.
//!
//! Library trees are marked [`Tree::library`]: their modules are never
//! tops, and dv-lint reports nothing in them.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use sv_source::FileList;
use sv_syntax::{Options, SyntaxKind, Tree};

/// Names of the design units a tree defines (at any depth, so nested
/// modules count).
pub fn defined(tree: &Tree) -> HashSet<String> {
    use SyntaxKind::*;
    let mut out = HashSet::new();
    for n in tree.preorder() {
        if matches!(tree.kind(n), ModuleDecl | InterfaceDecl | ProgramDecl | UdpDecl) {
            if let Some(name) = tree.child(n, Name) {
                out.insert(tree.bare_text(name));
            }
        }
    }
    out
}

/// Names a tree instantiates (`cell u1(...)`, and `bind` targets' units).
pub fn instantiated(tree: &Tree) -> BTreeSet<String> {
    use SyntaxKind::*;
    let mut out = BTreeSet::new();
    for n in tree.preorder() {
        if matches!(tree.kind(n), Instantiation | BindDirective) {
            if let Some((first, _)) = tree.child(n, DataType).and_then(|dt| tree.token_bounds(dt)) {
                out.insert(tree.tokens[first as usize].text.to_string());
            }
        }
    }
    out
}

/// The file `-y` directories give for `name`: `DIR/<name><ext>` for each
/// directory in order and each extension in order (just `DIR/<name>` when
/// there are none).
pub fn find_in_dirs(name: &str, dirs: &[PathBuf], exts: &[String]) -> Option<PathBuf> {
    let none = [String::new()];
    let exts = if exts.is_empty() { &none[..] } else { exts };
    dirs.iter().flat_map(|d| exts.iter().map(move |e| d.join(format!("{name}{e}")))).find(|p| p.is_file())
}

/// Parses the library files the sources need and appends them to `trees`,
/// marked as library trees. Returns how many were added.
pub fn add_library_trees(trees: &mut Vec<Tree>, list: &FileList, opts: &Options) -> usize {
    let dirs: Vec<PathBuf> = list.library_dirs.iter().map(|d| d.value.clone()).collect();
    let files: Vec<PathBuf> = list.library_files.iter().map(|f| f.value.clone()).collect();
    if dirs.is_empty() && files.is_empty() {
        return 0;
    }
    let mut have: HashSet<String> = HashSet::new();
    let mut wanted: BTreeSet<String> = BTreeSet::new();
    for t in trees.iter() {
        have.extend(defined(t));
        wanted.extend(instantiated(t));
    }
    let read: HashSet<PathBuf> = trees.iter().flat_map(|t| t.files.iter().cloned()).collect();
    let mut read: HashSet<PathBuf> = read.iter().map(|p| canon(p)).collect();
    // -v files are parsed once, when first needed, and kept until then.
    let mut pending: Vec<(PathBuf, Option<Tree>)> = files.into_iter().map(|f| (f, None)).collect();
    let mut added = 0;
    loop {
        let missing: Vec<String> = wanted.iter().filter(|n| !have.contains(*n)).cloned().collect();
        if missing.is_empty() {
            break;
        }
        let mut new: Vec<Tree> = Vec::new();
        for (path, slot) in pending.iter_mut() {
            if read.contains(&canon(path)) || !path.is_file() {
                continue;
            }
            let t = slot.get_or_insert_with(|| parse_lib(path, opts));
            let defs = defined(t);
            if missing.iter().any(|m| defs.contains(m)) {
                read.insert(canon(path));
                new.push(slot.take().expect("parsed above"));
            }
        }
        for t in &new {
            have.extend(defined(t));
        }
        for name in &missing {
            if have.contains(name) {
                continue;
            }
            let Some(p) = find_in_dirs(name, &dirs, &list.library_exts) else { continue };
            if !read.insert(canon(&p)) {
                continue;
            }
            let t = parse_lib(&p, opts);
            have.extend(defined(&t));
            new.push(t);
        }
        if new.is_empty() {
            break;
        }
        for t in &new {
            wanted.extend(instantiated(t));
        }
        added += new.len();
        trees.extend(new);
    }
    added
}

fn canon(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

fn parse_lib(path: &Path, opts: &Options) -> Tree {
    let mut t = sv_syntax::parse_file(path, opts.clone());
    t.library = true;
    t
}
