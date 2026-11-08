//! dv-lint's pattern results in tools/oracles/regexcheck.cpp's format, for
//! tools/gates/regex_gate.py: PATTERN<TAB>FLAGS<TAB>TEXT lines in, "err" or
//! "S M" (regex_search, regex_match) lines out.
//!
//!     cargo run --release -p dv-lint --example regexcheck < cases.tsv

use std::io::{BufRead, Write};

fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'\\' || i + 1 >= b.len() {
            out.push(b[i]);
            i += 1;
            continue;
        }
        i += 1;
        match b[i] {
            b't' => out.push(b'\t'),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b'x' if i + 2 < b.len() => {
                out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap_or(b'?'));
                i += 2;
            }
            n => out.push(n),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn main() {
    let stdin = std::io::stdin();
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    for line in stdin.lock().lines() {
        let line = line.unwrap_or_default();
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        let [pattern, flags, text] = parts[..] else {
            let _ = writeln!(out, "err");
            continue;
        };
        let _ = match dv_lint::pattern::Pattern::new(&unescape(pattern), flags == "i") {
            Ok(p) => {
                let t = unescape(text);
                writeln!(out, "{} {}", u8::from(p.is_match_anywhere(&t)), u8::from(p.is_full_match(&t)))
            }
            Err(_) => writeln!(out, "err"),
        };
    }
}
