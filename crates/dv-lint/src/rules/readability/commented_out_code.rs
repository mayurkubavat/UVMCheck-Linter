//! SV-RDB-003 (CommentedOutCode): Flags blocks of // comments that parse as
//! SystemVerilog statements or declarations (Sonar S125).

use crate::support::trivia::{library_file, split_trivia, Triv};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::HashMap;
use sv_syntax::Origin;

/// Each written token's trivia (split as slang does), with the token; and
/// the trivia after each file's last token, with no token.
fn token_trivia(c: &Cst) -> Vec<(Option<u32>, u32, Vec<Triv>)> {
    let t = c.tree;
    let mut out = Vec::new();
    let mut last: HashMap<u32, u32> = HashMap::new();
    for (i, tok) in t.tokens.iter().enumerate() {
        if tok.from_macro {
            continue;
        }
        let f = tok.origin.file;
        last.insert(f, tok.origin.offset + tok.text.len() as u32);
        if tok.trivia_start == tok.origin.offset {
            continue;
        }
        out.push((Some(i as u32), f, split_trivia(t.trivia(i as u32), tok.trivia_start)));
    }
    let mut ends: Vec<(u32, u32)> = last.into_iter().collect();
    if !t.sources.is_empty() && !ends.iter().any(|&(f, _)| f == 0) {
        ends.push((0, 0));
    }
    ends.sort_unstable();
    for (f, end) in ends {
        let src = &t.sources[f as usize];
        if (end as usize) < src.len() {
            out.push((None, f, split_trivia(&src[end as usize..], end)));
        }
    }
    out
}

fn looks_like_code_line(l: &str) -> bool {
    if l.is_empty() {
        return false;
    }
    if l.ends_with(';') {
        return true;
    }
    matches!(l, "begin" | "end" | "endtask" | "endfunction" | "join" | "join_none" | "join_any")
        || l.starts_with("end ")
        || l.starts_with("fork")
        || l.ends_with(" begin")
}

fn parses_cleanly(text: &str) -> bool {
    let wrappers = [("module m__; initial begin\n", "\nend\nendmodule\n"), ("class c__;\n", "\nendclass\n")];
    wrappers.iter().any(|(a, b)| {
        let tree = sv_syntax::parse_source(&format!("{a}{text}{b}"), "", Default::default());
        tree.errors.is_empty() && tree.pp_diagnostics.is_empty()
    })
}

fn trim_comment(s: &str) -> &str {
    s.trim_start_matches([' ', '\t']).trim_end_matches([' ', '\t', '\r', '\n'])
}

pub(crate) fn commented_out_code(c: &Cst, s: &mut Sink) {
    const MIN_LINES: usize = 2;
    let t = c.tree;
    for (tok, f, triv) in token_trivia(c) {
        if tok.is_some_and(|x| t.tokens[x as usize].from_macro) || library_file(c, f) {
            continue;
        }
        let src = &t.sources[f as usize];
        let mut group: Vec<&str> = Vec::new();
        let mut start = 0u32;
        let mut eols = 0;
        let flush = |group: &mut Vec<&str>, start: u32, s: &mut Sink| {
            let code = group.iter().filter(|l| looks_like_code_line(l)).count();
            if code >= MIN_LINES {
                let mut text = String::new();
                for l in group.iter() {
                    text.push_str(l);
                    text.push('\n');
                }
                if parses_cleanly(&text) {
                    s.add(
                        "SV-RDB-003",
                        "CommentedOutCode",
                        Severity::Info,
                        format!(
                            "{} lines of commented-out code. Version control keeps old code; \
                             delete it, or turn it into a switch if it is still needed.",
                            group.len()
                        ),
                        c.loc_at(Origin { file: f, offset: start }),
                    );
                }
            }
            group.clear();
        };
        for x in triv {
            match x {
                Triv::Line(a, e) => {
                    if group.is_empty() {
                        start = a;
                    }
                    group.push(trim_comment(&src[a as usize + 2..e as usize]));
                    eols = 0;
                }
                Triv::Eol => {
                    eols += 1;
                    if eols > 1 && !group.is_empty() {
                        flush(&mut group, start, s);
                    }
                }
                Triv::Space => {}
                Triv::Other => {
                    if !group.is_empty() {
                        flush(&mut group, start, s);
                    }
                }
            }
        }
        if !group.is_empty() {
            flush(&mut group, start, s);
        }
    }
}
