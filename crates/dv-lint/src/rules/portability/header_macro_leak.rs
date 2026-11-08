//! SV-MAC-001 (HeaderMacroLeak): Macros defined inside header files (.svh) must
//! be undef'd with `undef before the end of the file unless registered as
//! public API macros.

use std::collections::HashSet;
use crate::syntax::Cst;
use crate::{Severity, Sink};

pub(crate) fn header_macro_leak(c: &Cst, s: &mut Sink) {
    // A file of directives only has just the end-of-file token, which slang
    // places in the file itself.
    let file = c.first_tok(c.tree.root).map_or(0, |t| c.tree.tokens[t as usize].origin.file);
    let Some(path) = c.tree.files.get(file as usize) else { return };
    let full = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
    let full = full.to_string_lossy().to_string();
    if !full.ends_with(".svh") {
        return;
    }
    let src: &str = &c.tree.sources[file as usize];
    let bytes = src.as_bytes();
    let at_start = |pos: usize| pos == 0 || matches!(bytes[pos - 1], b'\n' | b' ' | b'\t');
    let name_at = |from: usize, stops: &[u8]| -> Option<(usize, String)> {
        let start = from + bytes[from..].iter().position(|b| !matches!(b, b' ' | b'\t'))?;
        let end = bytes[start..].iter().position(|b| stops.contains(b)).map_or(bytes.len(), |e| start + e);
        Some((start, src[start..end].to_string()))
    };
    let mut defined: Vec<(String, usize)> = Vec::new();
    let mut undefined: HashSet<String> = HashSet::new();
    for (pos, _) in src.match_indices("`define") {
        if !at_start(pos) {
            continue;
        }
        let Some((start, name)) = name_at(pos + 7, b" \t\r\n(") else { continue };
        let guard = name.ends_with("_SVH") || name.ends_with("_H") || name.ends_with("_INC") || name.ends_with("_GUARD");
        let public = name.starts_with("uvm_") || name.starts_with("UVM_");
        if !guard && !public && !name.is_empty() {
            defined.push((name, start));
        }
    }
    for (pos, _) in src.match_indices("`undef") {
        if !at_start(pos) {
            continue;
        }
        if let Some((_, name)) = name_at(pos + 6, b" \t\r\n") {
            undefined.insert(name);
        }
    }
    for (name, off) in defined {
        if undefined.contains(&name) {
            continue;
        }
        s.add(
            "SV-MAC-001",
            "HeaderMacroLeak",
            Severity::Warning,
            format!(
                "Macro `{name} is defined in header file '{full}' without a corresponding `undef before file end. \
                 Leaking preprocessor definitions into global compilation scope causes accidental macro collisions across packages."
            ),
            c.loc_at(sv_syntax::Origin { file, offset: off as u32 }),
        );
    }
}
