//! SV-STY-013 (IncludeGuard): Every included header file (.svh) must be wrapped
//! in standard macro include guards (`ifndef / `define / `endif) to prevent
//! circular definitions and compiler redefinition errors.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::source_layout::{at};

/// slang's full path of a file: absolute, with `.` and `..` resolved.
fn full_path(p: &std::path::Path) -> String {
    p.canonicalize()
        .or_else(|_| std::env::current_dir().map(|d| d.join(p)))
        .unwrap_or_else(|_| p.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

pub(crate) fn include_guard(c: &Cst, s: &mut Sink) {
    let t = c.tree;
    // The file of the tree's first token (after preprocessing), else the
    // end of the main file.
    let (file, loc) = match t.tokens.first() {
        Some(tok) if tok.from_macro => return,
        Some(tok) => (tok.origin.file, c.loc(0)),
        None if !t.mains.is_empty() => (t.mains[0], at(c, t.mains[0], t.sources[t.mains[0] as usize].len())),
        None => return,
    };
    let path = full_path(&t.files[file as usize]);
    if !path.ends_with(".svh") {
        return;
    }
    let text = &t.sources[file as usize];
    if text.contains("`ifndef") && text.contains("`define") && text.contains("`endif") {
        return;
    }
    s.add(
        "SV-STY-013",
        "IncludeGuard",
        Severity::Error,
        format!(
            "Header file '{path}' is missing standard macro include guards (`ifndef / `define / `endif). \
             By convention, all header files (.svh) must be protected by include guards to prevent multiple inclusion errors."
        ),
        loc,
    );
}
