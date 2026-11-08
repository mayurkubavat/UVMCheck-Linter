//! SV-STY-003 (TodoCommentTracker): Tracks unresolved TODO, FIXME, ICEBOX,
//! HACK, and XXX comment markers across the codebase.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_sema::fx::FxMap as HashMap;
use sv_syntax::Origin;

/// The comments in `text` (which starts at byte `base` of its file), as
/// byte ranges, with whether each is inside a `define. Strings are skipped.
fn comments(text: &str, base: u32) -> Vec<(u32, u32, bool)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut define_end = 0;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'`' if text[i..].starts_with("`define") => {
                // To the first line end not escaped by a backslash.
                let mut e = i;
                while e < b.len() {
                    if b[e] == b'\n' {
                        let line = text[..e].trim_end_matches('\r');
                        if !line.ends_with('\\') {
                            break;
                        }
                    }
                    e += 1;
                }
                define_end = e;
                i += 1;
            }
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' && b[i] != b'\n' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                let end = text[i..].find('\n').map_or(b.len(), |e| i + e);
                out.push((base + i as u32, base + end as u32, i < define_end));
                i = end;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let end = text[i + 2..].find("*/").map_or(b.len(), |e| i + 2 + e + 2);
                out.push((base + i as u32, base + end as u32, i < define_end));
                i = end;
            }
            _ => i += 1,
        }
    }
    out
}

const TAGS: [&str; 5] = ["TODO", "FIXME", "ICEBOX", "HACK", "XXX"];

/// SV-STY-003 reads the comments slang keeps as token trivia: not those in
/// a branch not taken, in a `define line, or right before a macro use
/// (the expansion's first token takes the use's place); but those inside
/// a macro body, after its first token, at each expansion.
pub(crate) fn todo_comments(c: &Cst, s: &mut Sink) {
    let t = c.tree;
    let mut uses: HashMap<u32, Vec<u32>> = HashMap::default();
    for u in &t.macro_uses {
        uses.entry(u.origin.file).or_default().push(u.origin.offset);
    }
    let skipped = |f: u32, p: u32| t.skipped.iter().any(|&(sf, a, e)| sf == f && a <= p && p < e);
    let report = |s: &mut Sink, f: u32, a: u32, e: u32, loc: dv_model::Loc| {
        let text = &t.sources[f as usize][a as usize..e as usize];
        let Some(tag) = TAGS.iter().find(|tag| text.contains(*tag)) else { return };
        let mut snippet = text.trim_end_matches(['\r', '\n']).to_string();
        if snippet.len() > 80 {
            let mut cut = 77;
            while !snippet.is_char_boundary(cut) {
                cut -= 1;
            }
            snippet = format!("{}...", &snippet[..cut]);
        }
        s.add(
            "SV-STY-003",
            "TodoCommentTracker",
            Severity::Info,
            format!(
                "Unresolved development tag '{tag}' found in comment: {snippet}. Track and resolve all temporary FIXME, TODO, and ICEBOX items prior to verification closure."
            ),
            loc,
        );
    };
    let mut lexed: HashMap<u32, crate::support::source_layout::Comments> = HashMap::default();
    // The last token written in each file, and its end.
    let mut last: HashMap<u32, (usize, u32)> = HashMap::default();
    for (i, tok) in t.tokens.iter().enumerate() {
        if tok.from_macro {
            // Comments before a token of a macro body (or argument), but
            // not the expansion's first token.
            let Some(prev) = i.checked_sub(1).map(|p| &t.tokens[p]) else { continue };
            if !prev.from_macro || prev.origin != tok.origin {
                continue;
            }
            let w = tok.written;
            let map = lexed
                .entry(w.file)
                .or_insert_with(|| crate::support::source_layout::comments_before(&t.sources[w.file as usize]));
            for &(a, e) in map.get(&w.offset).map_or(&[][..], Vec::as_slice) {
                report(s, w.file, a, e, c.loc(i as u32));
            }
            continue;
        }
        let f = tok.origin.file;
        last.insert(f, (i, tok.origin.offset + tok.text.len() as u32));
        if tok.trivia_start == tok.origin.offset {
            continue;
        }
        let triv = &t.sources[f as usize][tok.trivia_start as usize..tok.origin.offset as usize];
        let file_uses = uses.get(&f).map_or(&[][..], Vec::as_slice);
        for (a, e, in_define) in comments(triv, tok.trivia_start) {
            if in_define || skipped(f, a) || file_uses.iter().any(|&m| m > a && m < tok.origin.offset) {
                continue;
            }
            report(s, f, a, e, c.loc(i as u32));
        }
    }
    // Trivia after a file's last token goes with the next token, or with
    // the end of the main file.
    let mut ends: Vec<(u32, Option<(usize, u32)>)> = last.into_iter().map(|(f, e)| (f, Some(e))).collect();
    if !t.sources.is_empty() && !ends.iter().any(|&(f, _)| f == 0) {
        // A main file with no code at all.
        ends.push((0, None));
    }
    ends.sort_unstable();
    for (f, e) in ends {
        let src = &t.sources[f as usize];
        let end = e.map_or(0, |(_, end)| end);
        let at = match e.map(|(i, _)| i + 1).filter(|&n| n < t.tokens.len()) {
            Some(next) => c.loc(next as u32),
            None => c.loc_at(Origin { file: f, offset: src.len() as u32 }),
        };
        let file_uses = uses.get(&f).map_or(&[][..], Vec::as_slice);
        for (a, e, in_define) in comments(&src[end as usize..], end) {
            if in_define || skipped(f, a) || file_uses.iter().any(|&m| m > a) {
                continue;
            }
            report(s, f, a, e, at);
        }
    }
}
