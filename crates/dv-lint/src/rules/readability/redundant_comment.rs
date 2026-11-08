//! SV-RDB-009 (RedundantComment): Flags a one-line // comment directly above a
//! statement when every word of the comment (2 to 6 words, ignoring a, the, to,
//! ...) is already a word of the statement's names, e.g. '// set cfg' above
//! set_cfg().

use crate::support::trivia::{ident_words, is_library_tok, split_trivia, Triv};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::BTreeSet;
use sv_lexer::Kind;
use sv_syntax::Origin;
use sv_syntax::SyntaxKind::*;

const STOP: [&str; 16] =
    ["the", "a", "an", "to", "of", "and", "for", "in", "on", "with", "this", "that", "is", "it", "from", "into"];

pub(crate) fn redundant_comment(c: &Cst, s: &mut Sink) {
    let t = c.tree;
    for n in c.nodes(ExprStmt) {
        let Some(first) = c.first_tok(n) else { continue };
        if is_library_tok(c, first) {
            continue;
        }
        let tok = &t.tokens[first as usize];
        let triv = split_trivia(t.trivia(first), tok.trivia_start);
        let mut comment: Option<(u32, u32)> = None;
        let mut eols = 0;
        for x in &triv {
            match *x {
                Triv::Line(a, e) => {
                    comment = Some((a, e));
                    eols = 0;
                }
                Triv::Eol => eols += 1,
                Triv::Space => {}
                Triv::Other => comment = None,
            }
        }
        let Some((a, e)) = comment else { continue };
        if eols != 1 {
            continue;
        }
        let src = &t.sources[tok.origin.file as usize];
        let text = &src[a as usize + 2..e as usize];
        let mut words = Vec::new();
        let mut cur = String::new();
        for ch in text.bytes() {
            if ch.is_ascii_alphanumeric() {
                cur.push(ch.to_ascii_lowercase() as char);
            } else {
                if !cur.is_empty() && !STOP.contains(&cur.as_str()) {
                    words.push(cur.clone());
                }
                cur.clear();
            }
        }
        if !cur.is_empty() && !STOP.contains(&cur.as_str()) {
            words.push(cur);
        }
        if words.len() < 2 || words.len() > 6 {
            continue;
        }
        let mut code: BTreeSet<String> = BTreeSet::new();
        for tk in t.token_range(n) {
            if t.tokens[tk as usize].kind == Kind::Identifier {
                code.extend(ident_words(c.text_of(tk)));
            }
        }
        let covered = |w: &String| {
            code.iter().any(|c| {
                c == w || (c.len() >= 3 && w.len() >= 3 && (c.starts_with(w.as_str()) || w.starts_with(c.as_str())))
            })
        };
        if !words.iter().all(covered) {
            continue;
        }
        s.add(
            "SV-RDB-009",
            "RedundantComment",
            Severity::Info,
            "The comment only repeats the code below it. Say why the line is there (the protocol \
             rule, the workaround, the corner case), or remove the comment."
                .into(),
            c.loc_at(Origin { file: tok.origin.file, offset: a }),
        );
    }
}
