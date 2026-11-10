//! SV-STY-002 (MacroTrailingSemicolon): Flags unnecessary semicolons following
//! `uvm_* macro invocations that create empty statements in declarative or
//! procedural scopes.

use crate::support::source_layout::at;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::Origin;

pub(crate) fn macro_trailing_semicolon(c: &Cst, s: &mut Sink) {
    let t = c.tree;
    for u in &t.macro_uses {
        if !u.name.starts_with("uvm_") {
            continue;
        }
        let Some(end) = macro_use_end(c, &u.name, u.origin, u.function_like) else { continue };
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

/// The end of a macro use in its file: after the name, or, for a macro that
/// takes arguments, after the closing parenthesis of its argument list,
/// which may follow the name after whitespace.
fn macro_use_end(c: &Cst, name: &str, origin: Origin, function_like: bool) -> Option<usize> {
    let src = &c.tree.sources[origin.file as usize];
    let start = origin.offset as usize + 1 + name.len();
    if src.get(origin.offset as usize..start)? != format!("`{name}") {
        return None;
    }
    let b = src.as_bytes();
    let mut i = start;
    if function_like {
        while b.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
    }
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
