//! The UVM library is not the user's code. When a compile holds both the
//! library (`uvm_pkg.sv` and what it includes) and files outside it, what
//! the rules find in the library is not reported: neither in its files nor
//! in the bodies of its macros expanded in user code (`uvm_object_utils`
//! expands to routines without end labels, `uvm_field_int` to a method of
//! cognitive complexity 207). No configuration is needed; a project config's
//! `library_paths` names further libraries.
//!
//! A compile of the library alone (every file under it) is someone linting
//! the library: everything is reported.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::Finding;

fn real(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// The UVM library of a compile.
#[derive(Debug, Default)]
pub struct Library {
    /// The library's source folders; none when nothing is hidden.
    roots: Vec<PathBuf>,
    /// The macros its files define.
    macros: HashSet<String>,
}

/// The names `` `define``d in the SystemVerilog files under `dir`.
fn defined_under(dir: &Path, into: &mut HashSet<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            defined_under(&p, into);
        } else if p.extension().is_some_and(|x| x == "sv" || x == "svh" || x == "v" || x == "vh") {
            let Ok(text) = std::fs::read_to_string(&p) else { continue };
            for (i, _) in text.match_indices("`define") {
                let name: String = text[i + 7..]
                    .trim_start_matches([' ', '\t'])
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                    .collect();
                if !name.is_empty() {
                    into.insert(name);
                }
            }
        }
    }
}

/// The first macro a source line calls (`` `uvm_info(...)`` gives `uvm_info`).
fn first_macro(line: &str) -> Option<&str> {
    let at = line.find('`')? + 1;
    let rest = &line[at..];
    let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$')).unwrap_or(rest.len());
    (end > 0).then(|| &rest[..end])
}

impl Library {
    /// Finds the library among the compile's `sources` (the files named on
    /// the command line and in its file lists): the folder of each
    /// `uvm_pkg.sv` and everything under it. Empty when there is none, or
    /// when no source is outside it.
    pub fn detect(sources: &[PathBuf]) -> Library {
        let sources: Vec<PathBuf> = sources.iter().map(|p| real(p)).collect();
        let mut roots: Vec<PathBuf> = sources
            .iter()
            .filter(|p| p.file_name().is_some_and(|n| n == "uvm_pkg.sv"))
            .filter_map(|p| p.parent().map(Path::to_path_buf))
            .collect();
        roots.sort();
        roots.dedup();
        let user_code = sources.iter().any(|p| !roots.iter().any(|r| p.starts_with(r)));
        if roots.is_empty() || !user_code {
            return Library::default();
        }
        let mut macros = HashSet::new();
        for r in &roots {
            defined_under(r, &mut macros);
        }
        Library { roots, macros }
    }

    /// [`Self::detect`] for a command line: its sources, read as the compile
    /// reads them (`-f` lists, `$VAR`).
    pub fn of_args<S: AsRef<str>>(args: &[S]) -> Library {
        let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
        let list = svlang::source::FileList::read_args(
            args.iter().map(|a| a.as_ref()),
            &cwd,
            svlang::source::Dialect::Any,
            &svlang::source::ProcessEnv,
        );
        let sources: Vec<PathBuf> = list.sources.iter().map(|s| s.value.clone()).collect();
        Library::detect(&sources)
    }

    /// The library's source folders (none: nothing is hidden).
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Drops the findings in library files and, unless `macro_bodies`,
    /// those made by a library macro expanded in user code: in its body,
    /// or on an argument it was given. `files` are the compile's files by
    /// index. Returns how many were dropped.
    pub fn hide(&self, found: &mut Vec<Finding>, files: &[String], macro_bodies: bool) -> usize {
        if self.roots.is_empty() {
            return 0;
        }
        let inside: Vec<bool> = files
            .iter()
            .map(|f| !f.is_empty() && self.roots.iter().any(|r| real(Path::new(f)).starts_with(r)))
            .collect();
        let has = |file: u32| inside.get(file as usize).copied().unwrap_or(false);
        // Source lines of the files with expansions, read once.
        let mut texts: HashMap<u32, Vec<String>> = HashMap::new();
        let mut calls_library_macro = |file: u32, line: u32| -> bool {
            let lines = texts.entry(file).or_insert_with(|| {
                let path = files.get(file as usize).map(String::as_str).unwrap_or("");
                svlang::syntax::overlay::read(Path::new(path))
                    .map(|t| t.lines().map(str::to_string).collect())
                    .unwrap_or_default()
            });
            (line as usize)
                .checked_sub(1)
                .and_then(|i| lines.get(i))
                .and_then(|l| first_macro(l))
                .is_some_and(|m| self.macros.contains(m))
        };
        let before = found.len();
        found.retain(|f| match f.loc.expansion {
            // Reported where the macro is used; `loc.file` is where the
            // token was written (the macro's body, or an argument).
            Some((at, line)) => !has(at) && (macro_bodies || !(has(f.loc.file) || calls_library_macro(at, line))),
            None => !has(f.loc.file),
        });
        before - found.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_library_alone_is_not_hidden() {
        let sources = [PathBuf::from("/x/uvm/src/uvm_pkg.sv")];
        assert!(Library::detect(&sources).roots().is_empty());
    }

    #[test]
    fn the_library_beside_user_code_is() {
        let sources = [PathBuf::from("/x/uvm/src/uvm_pkg.sv"), PathBuf::from("/x/tb/tb.sv")];
        assert_eq!(Library::detect(&sources).roots(), [PathBuf::from("/x/uvm/src")]);
    }

    #[test]
    fn the_first_macro_of_a_line() {
        assert_eq!(first_macro("    `uvm_info(\"A\", \"b\", UVM_LOW)"), Some("uvm_info"));
        assert_eq!(first_macro("    x = 1;"), None);
    }
}
