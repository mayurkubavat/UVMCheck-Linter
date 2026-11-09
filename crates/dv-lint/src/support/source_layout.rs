//! Line buffers, comments, banners, end labels and define ranges, for the
//! style rules.

use sv_sema::fx::FxMap as HashMap;

use dv_model::Loc;

use sv_syntax::{NodeId, Origin};

use crate::syntax::Cst;

use crate::{Severity, Sink};

use sv_syntax::SyntaxKind::*;

/// Each file of the tree with its text as slang holds it (a trailing NUL).
/// A file included again whose include guard skips it all is not read
/// again by slang.
pub(crate) fn buffers<'t>(c: &Cst<'t>) -> impl Iterator<Item = (u32, String)> + 't {
    let t = c.tree;
    let prior = c.prior_files.clone();
    let mut used = vec![false; t.sources.len()];
    // Lexed here (not only written there, as macro bodies are).
    let mut lexed = vec![false; t.sources.len()];
    for tok in &t.tokens {
        used[tok.origin.file as usize] = true;
        used[tok.written.file as usize] = true;
        lexed[tok.origin.file as usize] = true;
    }
    (0..t.sources.len() as u32).filter_map(move |f| {
        let again = t.files[..f as usize].contains(&t.files[f as usize]);
        if again && !used[f as usize] && guarded(&t.sources[f as usize]) {
            return None;
        }
        // A file an earlier tree of the compile read that this tree takes
        // no token from: carried along, not read again.
        if prior.contains(&t.files[f as usize]) && !lexed[f as usize] {
            return None;
        }
        let mut s = t.sources[f as usize].to_string();
        s.push('\0');
        Some((f, s))
    })
}

/// The file's code starts with `ifndef (after comments and whitespace).
pub(crate) fn guarded(src: &str) -> bool {
    sv_lexer::lex(src)
        .iter()
        .find(|t| !t.kind.is_trivia())
        .is_some_and(|t| src[t.start as usize..].starts_with("`ifndef"))
}

/// Lines of `text` as (start, end without '\r', number), split on '\n'.
pub(crate) fn lines(text: &str) -> Vec<(usize, usize, usize)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let (mut start, mut num) = (0, 1);
    for i in 0..=b.len() {
        if i == b.len() || b[i] == b'\n' {
            let mut end = i;
            if end > start && b[end - 1] == b'\r' {
                end -= 1;
            }
            out.push((start, end, num));
            start = i + 1;
            num += 1;
        }
    }
    out
}

pub(crate) fn at(c: &Cst, file: u32, offset: usize) -> Loc {
    c.loc_at(Origin { file, offset: offset as u32 })
}

/// UVM-BAN-001/002 scan the main file of each tree (not included files)
/// line by line.
pub(crate) fn banned_macros(c: &Cst, s: &mut Sink, field: bool) {
    let Some(&main) = c.tree.mains.first() else {
        return;
    };
    let mut text = c.tree.sources[main as usize].to_string();
    text.push('\0');
    let mut offset = 0;
    while offset < text.len() {
        let nl = text[offset..].find('\n').map_or(text.len(), |e| offset + e);
        let line = &text[offset..nl];
        let trimmed = line.trim_start_matches([' ', '\t']);
        if !trimmed.is_empty() && !trimmed.starts_with("//") && !trimmed.starts_with("/*") {
            if field {
                let hit = line.find("`uvm_field_").or_else(|| {
                    line.find("`uvm_object_utils_begin").or_else(|| line.find("`uvm_component_utils_begin"))
                });
                if let Some(h) = hit {
                    s.add(
                        "UVM-BAN-001",
                        "UvmBannedFieldMacros",
                        Severity::Warning,
                        "The `uvm_field_* macro suite is banned in high-performance verification environments (Cliff Cummings / Verification Academy). \
                         Field macros introduce severe simulation overhead (3x-10x execution drag), uncontrollable deep copying, \
                         and opaque compile errors. Implement explicit 'do_copy', 'do_compare', and 'do_print' methods instead."
                            .into(),
                        at(c, main, offset + h),
                    );
                }
            } else if let Some(h) = line.find("`uvm_do") {
                let rest = &line[h + 7..];
                if rest.is_empty() || ["(", " ", "_with", "_on", "_pri"].iter().any(|p| rest.starts_with(p)) {
                    s.add(
                        "UVM-BAN-002",
                        "UvmBannedUvmDoMacros",
                        Severity::Warning,
                        "Macro '`uvm_do' is discouraged in modern sequence development. \
                         It obscures the sequence-driver protocol handshake and conceals transaction randomization. \
                         Use explicit driver handshake: 'start_item(req)' -> 'assert(req.randomize())' -> 'finish_item(req)'."
                            .into(),
                        at(c, main, offset + h),
                    );
                }
            }
        }
        offset = nl + 1;
    }
}

/// The comments in `text` (which starts at byte `base` of its file), as
/// byte ranges, with whether each is inside a `define. Strings are skipped.
/// (As in `syntax3`.)
pub(crate) fn comments(text: &str, base: u32) -> Vec<(u32, u32, bool)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut define_end = 0;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'`' if text[i..].starts_with("`define") => {
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

/// Each token's leading comments (start, end), by token offset.
pub(crate) type Comments = std::sync::Arc<HashMap<u32, Vec<(u32, u32)>>>;

/// Each token's leading comments, by token offset, from lexing `src`.
/// Shared: the same files are read by several rules.
pub(crate) fn comments_before(src: &str) -> Comments {
    if crate::shared::current() == 0 {
        return std::sync::Arc::new(build_comments_before(src));
    }
    crate::shared::get("comments_before", (src.as_ptr() as usize, src.len()), || build_comments_before(src))
}

fn build_comments_before(src: &str) -> HashMap<u32, Vec<(u32, u32)>> {
    let mut map = HashMap::default();
    let mut pending = Vec::new();
    for &tk in crate::shared::raw_lex(src).iter() {
        match tk.kind {
            sv_lexer::Kind::LineComment | sv_lexer::Kind::BlockComment => pending.push((tk.start, tk.start + tk.len)),
            k if k.is_trivia() => {}
            sv_lexer::Kind::Directive => pending.clear(),
            _ => {
                if !pending.is_empty() {
                    map.insert(tk.start, std::mem::take(&mut pending));
                }
            }
        }
    }
    map
}

/// Start of the last directive in trivia `text` (at byte `base`), or 0:
/// slang keeps the comments before a directive inside the directive.
pub(crate) fn last_directive(text: &str, base: u32) -> u32 {
    // A directive starts with a backtick.
    if !text.as_bytes().contains(&b'`') {
        return 0;
    }
    sv_lexer::lex(text)
        .iter()
        .filter(|t| t.kind == sv_lexer::Kind::Directive)
        .map(|t| base + t.start)
        .next_back()
        .unwrap_or(0)
}

/// The comments slang keeps as token trivia, each as (file, start, end)
/// with the location of the token that carries it: not those in a branch
/// not taken, in a `define line or right before a macro use; but those in
/// a macro body after its first token, at each expansion (see SV-STY-003).
pub(crate) fn trivia_comments(c: &Cst) -> Vec<(u32, u32, u32, Loc)> {
    crate::shared::get("trivia_comments", c.tree_key(), || build_trivia_comments(c)).to_vec()
}

pub(crate) fn build_trivia_comments(c: &Cst) -> Vec<(u32, u32, u32, Loc)> {
    let t = c.tree;
    let mut out = Vec::new();
    let mut uses: HashMap<u32, Vec<u32>> = HashMap::default();
    for u in &t.macro_uses {
        uses.entry(u.origin.file).or_default().push(u.origin.offset);
    }
    let skipped = |f: u32, p: u32| t.skipped.iter().any(|&(sf, a, e)| sf == f && a <= p && p < e);
    let mut lexed: HashMap<u32, Comments> = HashMap::default();
    let mut last: HashMap<u32, (usize, u32)> = HashMap::default();
    for (i, tok) in t.tokens.iter().enumerate() {
        if tok.from_macro {
            let Some(prev) = i.checked_sub(1).map(|p| &t.tokens[p]) else { continue };
            if !prev.from_macro || prev.origin != tok.origin {
                continue;
            }
            let w = tok.written;
            let map = lexed.entry(w.file).or_insert_with(|| comments_before(&t.sources[w.file as usize]));
            for &(a, e) in map.get(&w.offset).map_or(&[][..], Vec::as_slice) {
                out.push((w.file, a, e, c.loc(i as u32)));
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
        let directive = last_directive(triv, tok.trivia_start);
        for (a, e, in_define) in comments(triv, tok.trivia_start) {
            if in_define || a < directive || skipped(f, a) || file_uses.iter().any(|&m| m > a && m < tok.origin.offset)
            {
                continue;
            }
            out.push((f, a, e, c.loc(i as u32)));
        }
    }
    let mut ends: Vec<(u32, Option<(usize, u32)>)> = last.into_iter().map(|(f, e)| (f, Some(e))).collect();
    if !t.sources.is_empty() && !ends.iter().any(|&(f, _)| f == 0) {
        ends.push((0, None));
    }
    ends.sort_unstable();
    for (f, e) in ends {
        let src = &t.sources[f as usize];
        let end = e.map_or(0, |(_, end)| end);
        let loc = match e.map(|(i, _)| i + 1).filter(|&n| n < t.tokens.len()) {
            Some(next) => c.loc(next as u32),
            None => c.loc_at(Origin { file: f, offset: src.len() as u32 }),
        };
        let file_uses = uses.get(&f).map_or(&[][..], Vec::as_slice);
        let directive = last_directive(&src[end as usize..], end);
        for (a, e, in_define) in comments(&src[end as usize..], end) {
            if in_define || a < directive || skipped(f, a) || file_uses.iter().any(|&m| m > a) {
                continue;
            }
            out.push((f, a, e, loc));
        }
    }
    out
}

/// SV-STY-007 (dashes) and SV-STY-008 (equals): an all-`-` or all-`=` line
/// comment outside 70..=85 characters.
pub(crate) fn banners(c: &Cst, s: &mut Sink, dashes: bool) {
    for (f, a, e, loc) in trivia_comments(c) {
        let text = &c.tree.sources[f as usize][a as usize..e as usize];
        if !text.starts_with("//") {
            continue;
        }
        let text = text.trim_end_matches(['\r', '\n', ' ']);
        let n = text.len();
        let only = |ch: u8| text.as_bytes()[2..].iter().all(|&b| b == ch);
        // A comment only: rewriting it to the standard 78 columns changes nothing else.
        let fix = |ch: char| {
            let line: String =
                std::iter::once("//".to_string()).chain(std::iter::repeat_n(ch.to_string(), 76)).collect();
            c.file_fix(f, a as usize, a as usize + n, line, "Make the banner 78 characters wide")
        };
        if text.starts_with("//==") {
            if !dashes && only(b'=') && !(70..=85).contains(&n) {
                let fx = fix('=');
                s.add(
                    "SV-STY-008",
                    "SectionHeaderBanner",
                    Severity::Info,
                    format!(
                        "Non-standard section header banner width ({n} characters). Major section divider banners must standardize to 70–80 columns (e.g. 78 characters: //===...)."
                    ),
                    loc,
                )
                .fixes
                .push(fx);
            }
        } else if dashes && text.starts_with("//--") && only(b'-') && !(70..=85).contains(&n) {
            let fx = fix('-');
            s.add(
                "SV-STY-007",
                "MethodBannerStyle",
                Severity::Info,
                format!(
                    "Ragged or non-standard method/separator banner ({n} characters). In standard style guidelines, method separators must span 70–80 columns (e.g. 78 hyphens: //---...)."
                ),
                loc,
            )
            .fixes
            .push(fx);
        }
    }
}

/// slang's `FunctionDeclarationSyntax`: function and task declarations
/// with a body, with their `function`/`task` keyword.
pub(crate) fn routines(c: &Cst) -> Vec<(NodeId, u32)> {
    let mut out = Vec::new();
    for n in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)) {
        let Some(kw) = c.toks(n).find(|&t| matches!(c.text_of(t), "function" | "task")) else { continue };
        out.push((n, kw));
    }
    out
}

/// The routine name as slang prints it without whitespace.
pub(crate) fn routine_name(c: &Cst, n: NodeId) -> String {
    c.tree.child(n, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default()
}

/// The end keyword of a routine or class and its label's name token. A
/// missing end keyword is where slang puts it: right after the last token.
#[derive(Clone, Copy)]
pub(crate) enum End {
    Tok(u32),
    Missing(Loc),
}

pub(crate) fn end_and_label(c: &Cst, n: NodeId, end: &[&str]) -> Option<(End, Option<u32>)> {
    let label = c.tree.child(n, EndLabel).and_then(|l| c.toks(l).filter(|&t| c.text_of(t) != ":").last());
    match c.toks(n).filter(|&t| end.contains(&c.text_of(t))).last() {
        Some(e) => Some((End::Tok(e), label)),
        None => {
            let (_, last) = c.tree.token_bounds(n)?;
            let tok = &c.tree.tokens[last as usize];
            if tok.from_macro {
                return None;
            }
            let o = Origin { file: tok.origin.file, offset: tok.origin.offset + tok.text.len() as u32 };
            Some((End::Missing(c.loc_at(o)), label))
        }
    }
}

pub(crate) fn end_loc(c: &Cst, e: End) -> Loc {
    match e {
        End::Tok(t) => c.loc(t),
        End::Missing(l) => l,
    }
}

pub(crate) fn classes(c: &Cst) -> Vec<(NodeId, String)> {
    c.nodes(ClassDecl)
        .into_iter()
        .map(|n| (n, c.tree.child(n, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default()))
        .collect()
}

/// The `define bodies of a file: (start, end) byte ranges.
pub(crate) fn define_ranges(src: &str) -> Vec<(u32, u32)> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = src[from..].find("`define").map(|i| from + i) {
        let mut e = i;
        while e < b.len() {
            if b[e] == b'\n' && !src[..e].trim_end_matches('\r').ends_with('\\') {
                break;
            }
            e += 1;
        }
        out.push((i as u32, e as u32));
        from = e.max(i + 1);
    }
    out
}

/// slang's buffer of a token: its file for written code; for macro code,
/// the expansion (the outermost use) and where the text was written (the
/// `define body, or the use's arguments).
pub(crate) struct Buffers<'c, 't> {
    pub(crate) c: &'c Cst<'t>,
    pub(crate) defines: std::cell::RefCell<HashMap<u32, Vec<(u32, u32)>>>,
}

impl<'c, 't> Buffers<'c, 't> {
    pub(crate) fn new(c: &'c Cst<'t>) -> Self {
        Buffers { c, defines: Default::default() }
    }

    pub(crate) fn of(&self, t: u32) -> (bool, u32, u32, u32, u32) {
        let tok = &self.c.tree.tokens[t as usize];
        if !tok.from_macro {
            return (false, tok.origin.file, 0, 0, 0);
        }
        let w = tok.written;
        let mut defs = self.defines.borrow_mut();
        let ranges = defs.entry(w.file).or_insert_with(|| define_ranges(&self.c.tree.sources[w.file as usize]));
        let body = ranges.iter().find(|&&(a, e)| a <= w.offset && w.offset < e).map_or(u32::MAX, |&(a, _)| a);
        (true, tok.origin.file, tok.origin.offset, w.file, body)
    }

    /// slang's line of a token: of the outermost expansion for macro code.
    pub(crate) fn line(&self, t: u32) -> u32 {
        let l = self.c.loc(t);
        l.expansion.map_or(l.line, |(_, line)| line)
    }
}

/// The fix that writes the end label: `endfunction` becomes `endfunction : f`
/// (none when the end keyword is missing or comes from a macro).
pub(crate) fn end_label_fix(c: &Cst, e: End, label: &str) -> Option<crate::fix::Fix> {
    let End::Tok(t) = e else { return None };
    c.token_fix(t, t, format!("{} : {label}", c.text_of(t)), "Add matching end label")
}
