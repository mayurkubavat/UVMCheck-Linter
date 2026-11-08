//! SV-STY-013 (IncludeGuard): Every included header file (.svh) must be wrapped
//! in standard macro include guards (`ifndef / `define / `endif) to prevent
//! circular definitions and compiler redefinition errors.

use crate::support::source_layout::at;
use crate::syntax::Cst;
use crate::{Severity, Sink};

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
    )
    .fixes
    .push(guard_fix(c, file, &path, text));
}

/// Wraps the whole file in `ifndef/`define/`endif as one edit, so the
/// halves can never be applied apart. Unsafe: a header meant to be included
/// more than once (to stamp out code per define) stops working.
fn guard_fix(c: &Cst, file: u32, path: &str, text: &str) -> crate::fix::Fix {
    let text = text.strip_suffix('\0').unwrap_or(text);
    let base = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let name: String =
        base.chars().map(|ch| if ch.is_ascii_alphanumeric() { ch.to_ascii_uppercase() } else { '_' }).collect();
    let mut body = text.to_string();
    if !body.is_empty() && !body.ends_with('\n') {
        body.push('\n');
    }
    let mut f = c.file_fix(
        file,
        0,
        text.len(),
        format!("`ifndef {name}\n`define {name}\n\n{body}\n`endif // {name}\n"),
        &format!("Wrap the file in an `ifndef {name} include guard"),
    );
    f.safety = crate::fix::FixSafety::Unsafe;
    f
}
