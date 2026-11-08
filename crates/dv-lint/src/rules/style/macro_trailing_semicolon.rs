//! SV-STY-002 (MacroTrailingSemicolon): Flags unnecessary semicolons following
//! `uvm_* macro invocations that create empty statements in declarative or
//! procedural scopes.

use crate::support::source_layout::at;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::Origin;

pub(crate) fn macro_trailing_semicolon(c: &Cst, s: &mut Sink) {
    let t = c.tree;
    for u in &t.macro_uses {
        if !u.name.starts_with("uvm_") {
            continue;
        }
        let Some(end) = macro_use_end(c, &u.name, u.origin) else { continue };
        let src = t.sources[u.origin.file as usize].as_bytes();
        let mut cur = end;
        while cur < src.len() && (src[cur] == b' ' || src[cur] == b'\t') {
            cur += 1;
        }
        if cur < src.len() && src[cur] == b';' {
            s.add(
                "SV-STY-002",
                "MacroTrailingSemicolon",
                Severity::Warning,
                format!(
                    "Unnecessary semicolon ';' following macro '`{}'. In declarative and block scopes, trailing semicolons create empty statements.",
                    u.name
                ),
                at(c, u.origin.file, cur),
            )
            // The macros expand to complete statements or declarations, so
            // the ';' is an empty item and deleting it changes nothing.
            .fixes
            .push(c.file_fix(u.origin.file, cur, cur + 1, "", "Remove the semicolon after the macro call"));
        }
    }
}

/// The end of a macro use in its file: after the name, or after the
/// closing parenthesis of its arguments when the macro takes arguments and
/// they follow.
fn macro_use_end(c: &Cst, name: &str, origin: Origin) -> Option<usize> {
    let src = &c.tree.sources[origin.file as usize];
    let start = origin.offset as usize + 1 + name.len();
    if src.get(origin.offset as usize..start)? != format!("`{name}") {
        return None;
    }
    let b = src.as_bytes();
    // Arguments start right after the name, or after whitespace when the
    // macro has formal arguments; here: a '(' directly or after spaces.
    let mut i = start;
    if b.get(i) != Some(&b'(') {
        return Some(start);
    }
    let mut depth = 0;
    while i < b.len() {
        match b[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}
