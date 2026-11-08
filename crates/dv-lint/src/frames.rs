//! Code frames for the text report (`dv-lint --frames`): the source line of
//! a finding with a caret under the flagged token,
//!
//! ```text
//!     logic data_in;
//!           ^~~~~~~
//! ```
//!
//! The caret starts at the finding's column and runs over the token there:
//! an identifier, a number, a `` `macro `` or `$task` name, or, at an
//! operator, the operator character. Tabs in the line
//! are kept in the underline so it lines up in any editor. A finding with
//! no column (inside a macro expansion, reported at the call's line) shows
//! the line alone.

use std::collections::HashMap;

/// Files read so far, by name, as lines.
#[derive(Default)]
pub struct Sources {
    files: HashMap<String, Option<Vec<String>>>,
}

impl Sources {
    fn line(&mut self, file: &str, line: u32) -> Option<&str> {
        let lines = self
            .files
            .entry(file.to_string())
            .or_insert_with(|| std::fs::read(file).ok().map(|b| String::from_utf8_lossy(&b).lines().map(str::to_string).collect()));
        lines.as_ref()?.get((line as usize).checked_sub(1)?).map(String::as_str)
    }

    /// The frame for `file:line:column` (1-based; column 0 is "no column"),
    /// each line indented by four spaces and ending in a newline; empty
    /// when the line cannot be read.
    pub fn frame(&mut self, file: &str, line: u32, column: u32) -> String {
        let Some(text) = self.line(file, line) else { return String::new() };
        let text = text.trim_end();
        let mut out = format!("    {text}\n");
        if column == 0 {
            return out;
        }
        let start = (column as usize - 1).min(text.len());
        let len = token_len(&text[start..]).max(1);
        let pad: String = text[..start].chars().map(|c| if c == '\t' { '\t' } else { ' ' }).collect();
        out.push_str(&format!("    {pad}^{}\n", "~".repeat(len - 1)));
        out
    }
}

/// Length of the token that starts `rest`.
fn token_len(rest: &str) -> usize {
    let b = rest.as_bytes();
    let word = |i: usize| b.get(i).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c == b'$');
    let mut i = 0;
    if matches!(b.first(), Some(b'`' | b'$')) {
        i = 1;
    }
    while word(i) {
        i += 1;
    }
    if i == 0 {
        return rest.chars().next().map_or(0, char::len_utf8);
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_under_the_token() {
        let d = std::env::temp_dir().join(format!("dv-lint-frames-{}.sv", std::process::id()));
        std::fs::write(&d, "module m;\n\tlogic data_in;\n  `uvm_info(\"a\", \"b\", UVM_LOW)\nendmodule\n").unwrap();
        let f = d.display().to_string();
        let mut s = Sources::default();
        assert_eq!(s.frame(&f, 2, 8), "    \tlogic data_in;\n    \t      ^~~~~~~\n");
        assert_eq!(s.frame(&f, 3, 3), "      `uvm_info(\"a\", \"b\", UVM_LOW)\n      ^~~~~~~~~\n");
        // No column: the line alone; no such line: nothing.
        assert_eq!(s.frame(&f, 1, 0), "    module m;\n");
        assert_eq!(s.frame(&f, 9, 1), "");
        std::fs::remove_file(&d).ok();
    }
}
