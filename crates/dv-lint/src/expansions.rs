//! Findings in code a macro made say which macro, and where in its body.
//!
//! A finding in a macro's expansion is reported at the line that uses the
//! macro, where the code the rule saw is not written. [`name_macros`] adds
//! a note with the macro's name at the place in its body the finding is
//! about, so the reader does not have to expand the macro in their head.

use crate::Finding;
use dv_model::Loc;
use std::collections::HashMap;
use std::path::Path;

/// The macro whose `` `define`` covers line `line` (1-based) of `lines`: the
/// nearest `` `define`` above it that reaches it through `\` continuations.
fn defined_at(lines: &[String], line: u32) -> Option<&str> {
    let mut i = (line as usize).checked_sub(1)?;
    lines.get(i)?;
    loop {
        let text = lines[i].trim_start();
        if let Some(rest) = text.strip_prefix("`define") {
            let rest = rest.trim_start();
            let end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '$')).unwrap_or(rest.len());
            return (end > 0).then(|| &rest[..end]);
        }
        // The line above must continue onto this one.
        i = i.checked_sub(1)?;
        if !lines[i].trim_end().ends_with('\\') {
            return None;
        }
    }
}

/// Adds to each finding inside a macro's body a note at that place:
/// "in the expansion of macro 'NAME'". Findings on a macro's arguments
/// (written where the macro is used) and findings that already have a note
/// there are left as they are. `files` are the compile's files by index.
pub fn name_macros(found: &mut [Finding], files: &[String]) {
    let mut texts: HashMap<u32, Vec<String>> = HashMap::new();
    for f in found.iter_mut() {
        let Some(_) = f.loc.expansion else { continue };
        let lines = texts.entry(f.loc.file).or_insert_with(|| {
            let path = files.get(f.loc.file as usize).map(String::as_str).unwrap_or("");
            svlang::syntax::overlay::read(Path::new(path))
                .map(|t| t.lines().map(str::to_string).collect())
                .unwrap_or_default()
        });
        let Some(name) = defined_at(lines, f.loc.line) else { continue };
        let at = Loc { expansion: None, ..f.loc };
        if f.related.iter().any(|(l, _)| l.file == at.file && l.line == at.line && l.column == at.column) {
            continue;
        }
        f.related.push((at, format!("in the expansion of macro '{name}'")));
    }
}

#[cfg(test)]
mod tests {
    use super::defined_at;

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(str::to_string).collect()
    }

    #[test]
    fn a_body_line_belongs_to_the_define_above_it() {
        let l = lines("`define CHECK(x) \\\n  if (x) \\\n    $display(\"bad\");\nint a;\n");
        assert_eq!(defined_at(&l, 1), Some("CHECK"));
        assert_eq!(defined_at(&l, 3), Some("CHECK"));
    }

    #[test]
    fn code_after_a_macro_is_not_in_it() {
        let l = lines("`define CHECK(x) \\\n  if (x) $stop;\nint a;\n");
        assert_eq!(defined_at(&l, 3), None);
        assert_eq!(defined_at(&l, 9), None);
    }
}
