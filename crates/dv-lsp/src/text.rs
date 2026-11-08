//! Positions and URIs: byte offsets to LSP line/UTF-16 column and back,
//! and file:// URIs to paths.

use std::path::{Path, PathBuf};

/// Line starts of one text, for offset <-> position conversion.
#[derive(Clone, Debug, Default)]
pub struct LineIndex {
    starts: Vec<u32>,
}

impl LineIndex {
    pub fn new(text: &str) -> Self {
        let mut starts = vec![0];
        for (i, b) in text.bytes().enumerate() {
            if b == b'\n' {
                starts.push(i as u32 + 1);
            }
        }
        LineIndex { starts }
    }

    /// Byte offset -> (line, UTF-16 column).
    pub fn position(&self, text: &str, offset: u32) -> (u32, u32) {
        let offset = (offset as usize).min(text.len()) as u32;
        let line = match self.starts.binary_search(&offset) {
            Ok(l) => l,
            Err(l) => l - 1,
        };
        let start = self.starts[line] as usize;
        let mut end = offset as usize;
        while end > start && !text.is_char_boundary(end) {
            end -= 1;
        }
        let col: usize = text[start..end].chars().map(char::len_utf16).sum();
        (line as u32, col as u32)
    }

    /// (line, UTF-16 column) -> byte offset, clamped to the line.
    pub fn offset(&self, text: &str, line: u32, col: u32) -> u32 {
        let Some(&start) = self.starts.get(line as usize) else {
            return text.len() as u32;
        };
        let end = self.starts.get(line as usize + 1).map(|&e| e as usize).unwrap_or(text.len());
        let mut units = 0u32;
        for (i, c) in text[start as usize..end].char_indices() {
            if units >= col || c == '\n' {
                return start + i as u32;
            }
            units += c.len_utf16() as u32;
        }
        end as u32
    }
}

pub fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// The identifier around `offset` (a leading backtick or `$` included):
/// its start and end bytes.
pub fn word_at(text: &str, offset: u32) -> Option<(usize, usize)> {
    let b = text.as_bytes();
    let mut s = (offset as usize).min(b.len());
    let mut e = s;
    while s > 0 && is_ident_byte(b[s - 1]) {
        s -= 1;
    }
    while e < b.len() && is_ident_byte(b[e]) {
        e += 1;
    }
    if s > 0 && b[s - 1] == b'`' {
        s -= 1;
    }
    (e > s).then_some((s, e))
}

pub fn path_to_uri(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file://");
    if !s.starts_with('/') {
        out.push('/');
    }
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~:".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

pub fn uri_to_path(uri: &str) -> PathBuf {
    let rest = uri.strip_prefix("file://").unwrap_or(uri);
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
            if let Ok(v) = u8::from_str_radix(hex, 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    let mut s = String::from_utf8_lossy(&out).into_owned();
    // file:///c:/x on Windows
    if s.len() > 2 && s.as_bytes()[0] == b'/' && s.as_bytes()[2] == b':' {
        s.remove(0);
    }
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions() {
        let t = "ab\nc\u{e9}d\n\u{1F600}x";
        let li = LineIndex::new(t);
        assert_eq!(li.position(t, 0), (0, 0));
        assert_eq!(li.position(t, 3), (1, 0));
        let x = t.find('x').unwrap() as u32;
        assert_eq!(li.position(t, x), (2, 2));
        assert_eq!(li.offset(t, 2, 2), x);
        assert_eq!(li.offset(t, 0, 99), 2);
    }

    #[test]
    fn uris() {
        let p = Path::new("/Users/me/My Dir/a.sv");
        let u = path_to_uri(p);
        assert_eq!(u, "file:///Users/me/My%20Dir/a.sv");
        assert_eq!(uri_to_path(&u), p);
    }

    #[test]
    fn words() {
        let t = "x = `uvm_info(a.bc";
        assert_eq!(word_at(t, 6).map(|(s, e)| &t[s..e]), Some("`uvm_info"));
        assert_eq!(word_at(t, 18).map(|(s, e)| &t[s..e]), Some("bc"));
    }
}
