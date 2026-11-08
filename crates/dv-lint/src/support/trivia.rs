//! Identifier words, comments and library code, for the readability rules.



use sv_lexer::Kind;


use crate::syntax::Cst;




pub(crate) fn lower(s: &str) -> String {
    s.to_ascii_lowercase()
}

/// Words of an identifier: split on '_' and on lower-to-upper case
/// changes, digits dropped.
pub(crate) fn ident_words(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in name.chars() {
        if ch == '_' || ch.is_ascii_digit() {
            if !cur.is_empty() {
                out.push(lower(&cur));
            }
            cur.clear();
            continue;
        }
        if ch.is_ascii_uppercase() && cur.chars().last().is_some_and(|l| l.is_ascii_lowercase()) {
            out.push(lower(&cur));
            cur.clear();
        }
        cur.push(ch);
    }
    if !cur.is_empty() {
        out.push(lower(&cur));
    }
    out
}

/// The UVM library and the test stub are not the user's code: files named
/// uvm_*, and code from macros.
pub(crate) fn is_library_tok(c: &Cst, t: u32) -> bool {
    let tok = &c.tree.tokens[t as usize];
    if tok.from_macro {
        return true;
    }
    library_file(c, tok.origin.file)
}

pub(crate) fn library_file(c: &Cst, f: u32) -> bool {
    c.tree.files[f as usize]
        .file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with("uvm_"))
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Triv {
    /// A `//` comment (without its line end): start and end offsets.
    Line(u32, u32),
    Eol,
    Space,
    /// Anything else: block comments, directives, skipped text.
    Other,
}

/// The trivia slang gives a token, from the native trivia text `text` at
/// byte `base` of its file: what follows the last directive or macro use
/// (slang keeps the trivia before a directive inside the directive).
pub(crate) fn split_trivia(text: &str, base: u32) -> Vec<Triv> {
    let toks = sv_lexer::lex(text);
    let from = toks.iter().rposition(|t| !t.kind.is_trivia()).map_or(0, |i| i + 1);
    let mut out = Vec::new();
    if from > 0 {
        out.push(Triv::Other);
    }
    for t in &toks[from..] {
        let (a, e) = (base + t.start, base + t.start + t.len);
        match t.kind {
            Kind::LineComment => out.push(Triv::Line(a, e)),
            Kind::Newline => out.push(Triv::Eol),
            Kind::Whitespace => out.push(Triv::Space),
            _ => out.push(Triv::Other),
        }
    }
    out
}
