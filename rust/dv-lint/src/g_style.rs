//! Style rules (builder slice N6.3, group F): whitespace and line length,
//! include guards, banners, end labels, inline method size, one statement
//! per line, keyword spacing, macro semicolons and UVM naming suffixes.
//! Each is a port of its C++ twin under `src/rules/style/` (and the banned
//! macro scanner under `src/rules/deprecated/`); messages are verbatim.

use std::collections::HashMap;

use dv_model::Loc;
use sv_syntax::{NodeId, Origin};

use crate::syntax::Cst;
use crate::Check::Syntax;
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-STY-015", name: "TrailingWhitespace", check: Syntax(trailing_whitespace) },
        Rule { id: "SV-STY-016", name: "MaxLineLength", check: Syntax(max_line_length) },
        Rule { id: "SV-STY-013", name: "IncludeGuard", check: Syntax(include_guard) },
        Rule { id: "UVM-BAN-001", name: "UvmBannedFieldMacros", check: Syntax(banned_field_macros) },
        Rule { id: "UVM-BAN-002", name: "UvmBannedUvmDoMacros", check: Syntax(banned_do_macros) },
        Rule { id: "SV-STY-007", name: "MethodBannerStyle", check: Syntax(method_banner) },
        Rule { id: "SV-STY-008", name: "SectionHeaderBanner", check: Syntax(section_banner) },
        Rule { id: "SV-STY-009", name: "ExternMethodThreshold", check: Syntax(extern_method_threshold) },
        Rule { id: "SV-STY-005", name: "MandatoryMatchingEndLabel", check: Syntax(missing_end_label) },
        Rule { id: "SV-STY-006", name: "MismatchedEndLabel", check: Syntax(mismatched_end_label) },
        Rule { id: "SV-STY-011", name: "OneStatementPerLine", check: Syntax(one_statement_per_line) },
        Rule { id: "SV-STY-012", name: "SpaceKeywordParen", check: Syntax(space_keyword_paren) },
        Rule { id: "SV-STY-002", name: "MacroTrailingSemicolon", check: Syntax(macro_trailing_semicolon) },
        Rule { id: "SV-NAM-005", name: "UvmClassTypeSuffix", check: Syntax(uvm_class_suffix) },
        Rule { id: "UVM-STY-001", name: "UvmCreateStringMismatch", check: Syntax(create_string_mismatch) },
        Rule { id: "UVM-TLM-003", name: "UvmTlmPortSuffix", check: Syntax(tlm_port_suffix) },
    ]
}

// ---- source text (slang's buffers: each file of the tree, with the
// terminating NUL slang appends) ----

/// Each file of the tree with its text as slang holds it (a trailing NUL).
/// A file included again whose include guard skips it all is not read
/// again by slang.
fn buffers<'t>(c: &Cst<'t>) -> impl Iterator<Item = (u32, String)> + 't {
    let t = c.tree;
    let mut used = vec![false; t.sources.len()];
    for tok in &t.tokens {
        used[tok.origin.file as usize] = true;
        used[tok.written.file as usize] = true;
    }
    (0..t.sources.len() as u32).filter_map(move |f| {
        let again = t.files[..f as usize].contains(&t.files[f as usize]);
        if again && !used[f as usize] && guarded(&t.sources[f as usize]) {
            return None;
        }
        let mut s = t.sources[f as usize].to_string();
        s.push('\0');
        Some((f, s))
    })
}

/// The file's code starts with `ifndef (after comments and whitespace).
fn guarded(src: &str) -> bool {
    sv_lexer::lex(src)
        .iter()
        .find(|t| !t.kind.is_trivia())
        .is_some_and(|t| src[t.start as usize..].starts_with("`ifndef"))
}

/// Lines of `text` as (start, end without '\r', number), split on '\n'.
fn lines(text: &str) -> Vec<(usize, usize, usize)> {
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

fn at(c: &Cst, file: u32, offset: usize) -> Loc {
    c.loc_at(Origin { file, offset: offset as u32 })
}

fn trailing_whitespace(c: &Cst, s: &mut Sink) {
    for (f, text) in buffers(c) {
        if text.len() == 1 {
            continue;
        }
        let b = text.as_bytes();
        for (start, end, num) in lines(&text) {
            let mut ws = end;
            while ws > start && (b[ws - 1] == b' ' || b[ws - 1] == b'\t') {
                ws -= 1;
            }
            if ws < end {
                s.add(
                    "SV-STY-015",
                    "TrailingWhitespace",
                    Severity::Info,
                    format!(
                        "Line {num} contains {} trailing whitespace character(s). Trailing whitespace creates spurious git diffs and violates formatting hygiene.",
                        end - ws
                    ),
                    at(c, f, ws),
                );
            }
        }
    }
}

fn max_line_length(c: &Cst, s: &mut Sink) {
    const MAX: usize = 120;
    for (f, text) in buffers(c) {
        if text.len() == 1 {
            continue;
        }
        for (start, end, num) in lines(&text) {
            let len = end - start;
            if len > MAX {
                s.add(
                    "SV-STY-016",
                    "MaxLineLength",
                    Severity::Info,
                    format!(
                        "Line {num} length ({len} characters) exceeds maximum recommended threshold of {MAX} characters. Wrap long expressions or break statements across lines."
                    ),
                    at(c, f, start + MAX),
                );
            }
        }
    }
}

/// slang's full path of a file: absolute, with `.` and `..` resolved.
fn full_path(p: &std::path::Path) -> String {
    p.canonicalize()
        .or_else(|_| std::env::current_dir().map(|d| d.join(p)))
        .unwrap_or_else(|_| p.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn include_guard(c: &Cst, s: &mut Sink) {
    let t = c.tree;
    // The file of the tree's first token (after preprocessing), else the
    // end of the main file.
    let (file, loc) = match t.tokens.first() {
        Some(tok) if tok.from_macro => return,
        Some(tok) => (tok.origin.file, c.loc(0)),
        None if !t.sources.is_empty() => (0, at(c, 0, t.sources[0].len())),
        None => return,
    };
    let path = full_path(&t.files[file as usize]);
    if !path.ends_with(".svh") {
        return;
    }
    let text = &t.sources[file as usize];
    if text.contains("`ifndef") && text.contains("`define") && text.contains("`endif") {
        return;
    }
    s.add(
        "SV-STY-013",
        "IncludeGuard",
        Severity::Error,
        format!(
            "Header file '{path}' is missing standard macro include guards (`ifndef / `define / `endif). \
             By convention, all header files (.svh) must be protected by include guards to prevent multiple inclusion errors."
        ),
        loc,
    );
}

/// UVM-BAN-001/002 scan the main file of each tree (not included files)
/// line by line.
fn banned_macros(c: &Cst, s: &mut Sink, field: bool) {
    if c.tree.sources.is_empty() {
        return;
    }
    let mut text = c.tree.sources[0].to_string();
    text.push('\0');
    let mut offset = 0;
    while offset < text.len() {
        let nl = text[offset..].find('\n').map_or(text.len(), |e| offset + e);
        let line = &text[offset..nl];
        let trimmed = line.trim_start_matches([' ', '\t']);
        if !trimmed.is_empty() && !trimmed.starts_with("//") && !trimmed.starts_with("/*") {
            if field {
                let hit = line
                    .find("`uvm_field_")
                    .or_else(|| line.find("`uvm_object_utils_begin").or_else(|| line.find("`uvm_component_utils_begin")));
                if let Some(h) = hit {
                    s.add(
                        "UVM-BAN-001",
                        "UvmBannedFieldMacros",
                        Severity::Warning,
                        "The `uvm_field_* macro suite is banned in high-performance verification environments (Cliff Cummings / Verification Academy). \
                         Field macros introduce severe simulation overhead (3x-10x execution drag), uncontrollable deep copying, \
                         and opaque compile errors. Implement explicit 'do_copy', 'do_compare', and 'do_print' methods instead."
                            .into(),
                        at(c, 0, offset + h),
                    );
                }
            } else if let Some(h) = line.find("`uvm_do") {
                let rest = &line[h + 7..];
                if rest.is_empty()
                    || ["(", " ", "_with", "_on", "_pri"].iter().any(|p| rest.starts_with(p))
                {
                    s.add(
                        "UVM-BAN-002",
                        "UvmBannedUvmDoMacros",
                        Severity::Warning,
                        "Macro '`uvm_do' is discouraged in modern sequence development. \
                         It obscures the sequence-driver protocol handshake and conceals transaction randomization. \
                         Use explicit driver handshake: 'start_item(req)' -> 'assert(req.randomize())' -> 'finish_item(req)'."
                            .into(),
                        at(c, 0, offset + h),
                    );
                }
            }
        }
        offset = nl + 1;
    }
}

fn banned_field_macros(c: &Cst, s: &mut Sink) {
    banned_macros(c, s, true);
}

fn banned_do_macros(c: &Cst, s: &mut Sink) {
    banned_macros(c, s, false);
}

// ---- comments slang keeps as token trivia ----

/// The comments in `text` (which starts at byte `base` of its file), as
/// byte ranges, with whether each is inside a `define. Strings are skipped.
/// (As in `syntax3`.)
fn comments(text: &str, base: u32) -> Vec<(u32, u32, bool)> {
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

/// Each token's leading comments, by token offset, from lexing `src`.
fn comments_before(src: &str) -> HashMap<u32, Vec<(u32, u32)>> {
    let mut map = HashMap::new();
    let mut pending = Vec::new();
    for tk in sv_lexer::lex(src) {
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
fn last_directive(text: &str, base: u32) -> u32 {
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
    let t = c.tree;
    let mut out = Vec::new();
    let mut uses: HashMap<u32, Vec<u32>> = HashMap::new();
    for u in &t.macro_uses {
        uses.entry(u.origin.file).or_default().push(u.origin.offset);
    }
    let skipped = |f: u32, p: u32| t.skipped.iter().any(|&(sf, a, e)| sf == f && a <= p && p < e);
    let mut lexed: HashMap<u32, HashMap<u32, Vec<(u32, u32)>>> = HashMap::new();
    let mut last: HashMap<u32, (usize, u32)> = HashMap::new();
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
            if in_define || a < directive || skipped(f, a) || file_uses.iter().any(|&m| m > a && m < tok.origin.offset) {
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
fn banners(c: &Cst, s: &mut Sink, dashes: bool) {
    for (f, a, e, loc) in trivia_comments(c) {
        let text = &c.tree.sources[f as usize][a as usize..e as usize];
        if !text.starts_with("//") {
            continue;
        }
        let text = text.trim_end_matches(['\r', '\n', ' ']);
        let n = text.len();
        let only = |ch: u8| text.as_bytes()[2..].iter().all(|&b| b == ch);
        if text.starts_with("//==") {
            if !dashes && only(b'=') && !(70..=85).contains(&n) {
                s.add(
                    "SV-STY-008",
                    "SectionHeaderBanner",
                    Severity::Info,
                    format!(
                        "Non-standard section header banner width ({n} characters). Major section divider banners must standardize to 70–80 columns (e.g. 78 characters: //===...)."
                    ),
                    loc,
                );
            }
        } else if dashes && text.starts_with("//--") && only(b'-') && !(70..=85).contains(&n) {
            s.add(
                "SV-STY-007",
                "MethodBannerStyle",
                Severity::Info,
                format!(
                    "Ragged or non-standard method/separator banner ({n} characters). In standard style guidelines, method separators must span 70–80 columns (e.g. 78 hyphens: //---...)."
                ),
                loc,
            );
        }
    }
}

fn method_banner(c: &Cst, s: &mut Sink) {
    banners(c, s, true);
}

fn section_banner(c: &Cst, s: &mut Sink) {
    banners(c, s, false);
}

// ---- routines and classes ----

/// slang's `FunctionDeclarationSyntax`: function and task declarations
/// with a body, with their `function`/`task` keyword.
fn routines(c: &Cst) -> Vec<(NodeId, u32)> {
    let mut out = Vec::new();
    for n in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)) {
        let Some(kw) = c.toks(n).find(|&t| matches!(c.text_of(t), "function" | "task")) else { continue };
        out.push((n, kw));
    }
    out
}

/// The routine name as slang prints it without whitespace.
fn routine_name(c: &Cst, n: NodeId) -> String {
    c.tree.child(n, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default()
}

fn extern_method_threshold(c: &Cst, s: &mut Sink) {
    const THRESHOLD: u32 = 5;
    for (n, kw) in routines(c) {
        if !c.parent(n).is_some_and(|p| c.kind(p) == ClassDecl) {
            continue;
        }
        if c.toks(n).any(|t| c.text_of(t) == "extern") {
            continue;
        }
        let Some((_, last)) = c.tree.token_bounds(n) else { continue };
        let lines = Buffers::new(c);
        let (a, b) = (lines.line(kw), lines.line(last));
        let count = if b >= a { b - a + 1 } else { 1 };
        if count > THRESHOLD {
            s.add(
                "SV-STY-009",
                "ExternMethodThreshold",
                Severity::Warning,
                format!(
                    "Method '{}' implemented inline inside class spans {count} lines (threshold: {THRESHOLD} lines). In common SystemVerilog style guidelines, large method implementations should be moved outside the class declaration using 'extern' to maintain a clean class API.",
                    routine_name(c, n)
                ),
                c.loc(kw),
            );
        }
    }
}

/// The end keyword of a routine or class and its label's name token. A
/// missing end keyword is where slang puts it: right after the last token.
enum End {
    Tok(u32),
    Missing(Loc),
}

fn end_and_label(c: &Cst, n: NodeId, end: &[&str]) -> Option<(End, Option<u32>)> {
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

fn end_loc(c: &Cst, e: End) -> Loc {
    match e {
        End::Tok(t) => c.loc(t),
        End::Missing(l) => l,
    }
}

fn classes(c: &Cst) -> Vec<(NodeId, String)> {
    c.nodes(ClassDecl)
        .into_iter()
        .map(|n| (n, c.tree.child(n, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default()))
        .collect()
}

fn missing_end_label(c: &Cst, s: &mut Sink) {
    for (n, _) in routines(c) {
        let Some((end, None)) = end_and_label(c, n, &["endfunction", "endtask"]) else { continue };
        let name = routine_name(c, n);
        s.add(
            "SV-STY-005",
            "MandatoryMatchingEndLabel",
            Severity::Info,
            format!(
                "Missing explicit end label on routine '{name}'. By convention, routines must terminate with a matching label ('endfunction : {name}' or 'endtask : {name}')."
            ),
            end_loc(c, end),
        );
    }
    for (n, name) in classes(c) {
        let Some((end, None)) = end_and_label(c, n, &["endclass"]) else { continue };
        s.add(
            "SV-STY-005",
            "MandatoryMatchingEndLabel",
            Severity::Info,
            format!(
                "Missing explicit end label on class '{name}'. By convention, classes must terminate with a matching label ('endclass : {name}')."
            ),
            end_loc(c, end),
        );
    }
}

fn mismatched_end_label(c: &Cst, s: &mut Sink) {
    for (n, _) in routines(c) {
        let Some((_, Some(label))) = end_and_label(c, n, &["endfunction", "endtask"]) else { continue };
        let name = routine_name(c, n);
        let l = c.text_of(label);
        if l == name || name.ends_with(&format!("::{l}")) {
            continue;
        }
        s.add(
            "SV-STY-006",
            "MismatchedEndLabel",
            Severity::Warning,
            format!(
                "Mismatched end label ':{l}' does not match routine name '{name}'. Update the label to match the routine declaration."
            ),
            c.loc(label),
        );
    }
    for (n, name) in classes(c) {
        let Some((_, Some(label))) = end_and_label(c, n, &["endclass"]) else { continue };
        let l = c.text_of(label);
        if l == name {
            continue;
        }
        s.add(
            "SV-STY-006",
            "MismatchedEndLabel",
            Severity::Warning,
            format!(
                "Mismatched end label ':{l}' does not match class name '{name}'. Update the label to match the class declaration."
            ),
            c.loc(label),
        );
    }
}

// ---- statements ----

/// The `define bodies of a file: (start, end) byte ranges.
fn define_ranges(src: &str) -> Vec<(u32, u32)> {
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
    c: &'c Cst<'t>,
    defines: std::cell::RefCell<HashMap<u32, Vec<(u32, u32)>>>,
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
        let ranges = defs
            .entry(w.file)
            .or_insert_with(|| define_ranges(&self.c.tree.sources[w.file as usize]));
        let body = ranges
            .iter()
            .find(|&&(a, e)| a <= w.offset && w.offset < e)
            .map_or(u32::MAX, |&(a, _)| a);
        (true, tok.origin.file, tok.origin.offset, w.file, body)
    }

    /// slang's line of a token: of the outermost expansion for macro code.
    pub(crate) fn line(&self, t: u32) -> u32 {
        let l = self.c.loc(t);
        l.expansion.map_or(l.line, |(_, line)| line)
    }
}

/// The first token slang gives a statement. A statement the parser could
/// not read (tokens only, no expression) is, after a stray `.`, slang's
/// empty statement at its `;`; other such statements are left out.
fn recovered_first(c: &Cst, n: NodeId) -> Option<u32> {
    let (a, e) = c.tree.token_bounds(n)?;
    if c.kind(n) == ExprStmt && c.tree.children(n).next().is_none() {
        return (c.text_of(a) == "." && c.text_of(e) == ";").then_some(e);
    }
    Some(a)
}

fn check_items(c: &Cst, s: &mut Sink, b: &Buffers, items: &[NodeId]) {
    let mut last_reported = 0;
    for w in items.windows(2) {
        let (Some(cur), Some(next)) = (c.tree.token_bounds(w[0]).map(|x| x.1), recovered_first(c, w[1])) else {
            continue;
        };
        if b.of(cur) != b.of(next) {
            continue;
        }
        let l = b.line(cur);
        if l == b.line(next) && l != last_reported {
            last_reported = l;
            s.add(
                "SV-STY-011",
                "OneStatementPerLine",
                Severity::Warning,
                format!(
                    "Multiple procedural statements found on line {l}. By convention, each statement must reside on its own line to ensure readable diffs and deterministic interactive debugging."
                ),
                c.loc(next),
            );
        }
    }
}

fn one_statement_per_line(c: &Cst, s: &mut Sink) {
    let b = Buffers::new(c);
    for n in c.nodes(SeqBlock).into_iter().chain(c.nodes(ParBlock)).chain(c.nodes(Body)) {
        let items = c.items(n);
        if items.len() >= 2 {
            check_items(c, s, &b, &items);
        }
    }
}

fn space_keyword_paren(c: &Cst, s: &mut Sink) {
    let b = Buffers::new(c);
    let check = |s: &mut Sink, kw: u32, open: u32| {
        if b.of(kw) != b.of(open) {
            return;
        }
        let (k, o) = (&c.tree.tokens[kw as usize], &c.tree.tokens[open as usize]);
        let (ko, oo) = if k.from_macro { (k.written.offset, o.written.offset) } else { (k.origin.offset, o.origin.offset) };
        if ko + k.text.len() as u32 == oo {
            let kw_text = c.text_of(kw);
            s.add(
                "SV-STY-012",
                "SpaceKeywordParen",
                Severity::Info,
                format!(
                    "Missing space between '{kw_text}' keyword and opening parenthesis '('. Common SystemVerilog formatting requires a space (e.g. '{kw_text} (...)')."
                ),
                c.loc(kw),
            );
        }
    };
    let pairs = |n: NodeId, kws: &[&str]| -> Option<(u32, u32)> {
        let kw = c.toks(n).find(|&t| kws.contains(&c.text_of(t)))?;
        let open = c.toks(n).find(|&t| t > kw && c.text_of(t) == "(")?;
        Some((kw, open))
    };
    let mut found = Vec::new();
    for n in c.nodes(IfStmt) {
        found.extend(pairs(n, &["if"]));
    }
    for n in c.nodes(LoopStmt) {
        found.extend(pairs(n, &["while", "repeat", "for", "foreach"]));
    }
    for n in c.nodes(DoWhileStmt) {
        found.extend(pairs(n, &["while"]));
    }
    for n in c.nodes(CaseStmt) {
        found.extend(pairs(n, &["case", "casex", "casez"]));
    }
    for (kw, open) in found {
        check(s, kw, open);
    }
}

fn macro_trailing_semicolon(c: &Cst, s: &mut Sink) {
    let t = c.tree;
    for u in &t.macro_uses {
        if !u.name.starts_with("uvm_") {
            continue;
        }
        let Some(end) = macro_use_end(c, &u.name, u.origin) else { continue };
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
            );
        }
    }
}

/// The end of a macro use in its file: after the name, or after the
/// closing parenthesis of its arguments when the macro takes arguments and
/// they follow.
fn macro_use_end(c: &Cst, name: &str, origin: Origin) -> Option<usize> {
    let src = &c.tree.sources[origin.file as usize];
    let start = origin.offset as usize + 1 + name.len();
    if src.get(origin.offset as usize..start)? != format!("`{name}") {
        return None;
    }
    let b = src.as_bytes();
    // Arguments start right after the name, or after whitespace when the
    // macro has formal arguments; here: a '(' directly or after spaces.
    let mut i = start;
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

// ---- UVM naming ----

const CLASS_SUFFIXES: [(&str, &[&str], &str); 8] = [
    ("uvm_sequence_item", &["_item", "_tr", "_trans", "_pkt", "Item", "Transaction", "Packet", "Tr"], "'_item' or '_tr'"),
    ("uvm_sequence", &["_seq", "_sequence", "_vseq", "Seq", "Sequence", "Vseq"], "'_seq'"),
    ("uvm_driver", &["_driver", "_drv", "Driver", "Drv"], "'_driver' or '_drv'"),
    ("uvm_monitor", &["_monitor", "_mon", "Monitor", "Mon"], "'_monitor' or '_mon'"),
    ("uvm_scoreboard", &["_scoreboard", "_scb", "Scoreboard", "Scb"], "'_scoreboard' or '_scb'"),
    ("uvm_agent", &["_agent", "_agt", "Agent", "Agt"], "'_agent' or '_agt'"),
    ("uvm_env", &["_env", "Env"], "'_env'"),
    ("uvm_test", &["_test", "Test"], "'_test'"),
];

fn uvm_class_suffix(c: &Cst, s: &mut Sink) {
    for (n, name) in classes(c) {
        let Some(ext) = c.tree.child(n, Header).and_then(|h| c.tree.child(h, Extends)) else { continue };
        let Some(ty) = c.tree.child(ext, DataType) else { continue };
        let base = c.last_text(ty);
        if name.starts_with("uvm_") {
            continue;
        }
        let pick = CLASS_SUFFIXES.iter().find(|(b, _, _)| match *b {
            "uvm_sequence" => base.contains("uvm_sequence") && !base.contains("sequence_item"),
            b => base.contains(b),
        });
        let Some((_, suffixes, rec)) = pick else { continue };
        if suffixes.iter().any(|x| name.ends_with(x)) {
            continue;
        }
        let Some(nm) = c.tree.child(n, Name).and_then(|x| c.first_tok(x)) else { continue };
        s.add(
            "SV-NAM-005",
            "UvmClassTypeSuffix",
            Severity::Warning,
            format!(
                "Class '{name}' extends '{base}' but does not end with recommended verification suffix {rec}. Standardizing UVM class suffixes improves component discoverability and codebase readability."
            ),
            c.loc(nm),
        );
    }
}

fn create_string_mismatch(c: &Cst, s: &mut Sink) {
    for n in c.nodes(BinaryExpr) {
        let Some(op) = c.toks(n).next() else { continue };
        if c.text_of(op) != "=" {
            continue;
        }
        let mut sides = c.tree.children(n);
        let (Some(l), Some(r)) = (sides.next(), sides.next()) else { continue };
        let var = match c.kind(l) {
            NameExpr if c.tree.token_bounds(l).is_some_and(|(a, b)| a == b) => c.last_text(l).to_string(),
            MemberExpr => c.last_text(l).to_string(),
            _ => String::new(),
        };
        if var.is_empty() || c.kind(r) != CallExpr {
            continue;
        }
        let Some(callee) = c.tree.children(r).next() else { continue };
        let callee_text: String = c.text(callee).chars().filter(|ch| !ch.is_whitespace()).collect();
        if !(callee_text.ends_with("::create") || callee_text.ends_with(".create") || callee_text == "create") {
            continue;
        }
        let Some(args) = c.tree.child(r, ArgList) else { continue };
        let Some(arg0) = c.items(args).into_iter().find(|&x| c.kind(x) == Arg) else { continue };
        let Some(expr) = c.items(arg0).into_iter().next() else { continue };
        let raw = c.trimmed(expr);
        if raw.len() < 2 || !raw.starts_with('"') || !raw.ends_with('"') {
            continue;
        }
        let val = &raw[1..raw.len() - 1];
        let ok = val == var
            || var.strip_prefix("m_") == Some(val)
            || val.strip_prefix("m_") == Some(var.as_str());
        if ok {
            continue;
        }
        let Some(first) = c.first_tok(expr) else { continue };
        s.add(
            "UVM-STY-001",
            "UvmCreateStringMismatch",
            Severity::Warning,
            format!(
                "UVM factory create instance string '\"{val}\"' does not match target variable identifier '{var}'. In UVM guidelines, instance names must match the variable name to ensure predictable uvm_config_db scoping."
            ),
            c.loc(first),
        );
    }
}

fn tlm_port_suffix(c: &Cst, s: &mut Sink) {
    for n in c.nodes(DataDecl) {
        let Some(ty) = c.tree.child(n, DataType) else { continue };
        let Some(first) = c.first_tok(ty) else { continue };
        let base = c.text_of(first);
        let (suffix, desc) = if base == "uvm_analysis_port" {
            ("_ap", "'_ap' or '_port'")
        } else if base.contains("analysis_imp") || base.ends_with("_imp") {
            ("_imp", "'_imp'")
        } else if base.contains("analysis_export") || base.ends_with("_export") {
            ("_export", "'_export' or '_exp'")
        } else if base.contains("tlm_fifo") || base.ends_with("_fifo") {
            ("_fifo", "'_fifo'")
        } else if base.contains("uvm_") && base.ends_with("_port") {
            ("_port", "'_port'")
        } else {
            continue;
        };
        for d in c.tree.children(n).filter(|&d| c.kind(d) == Declarator) {
            let Some(nm) = c.tree.child(d, Name) else { continue };
            let var = c.tree.bare_text(nm);
            if var.is_empty() {
                continue;
            }
            let ok = match suffix {
                "_ap" => var.ends_with("_ap") || var.ends_with("_port") || var == "ap",
                "_export" => var.ends_with("_export") || var.ends_with("_exp") || var == "export" || var == "exp",
                "_fifo" => var.ends_with("_fifo") || var == "fifo",
                "_imp" => var.ends_with("_imp") || var == "imp",
                x => var.ends_with(x),
            };
            if ok {
                continue;
            }
            let Some(t) = c.first_tok(nm) else { continue };
            s.add(
                "UVM-TLM-003",
                "UvmTlmPortSuffix",
                Severity::Warning,
                format!(
                    "TLM object '{var}' of type '{base}' violates naming conventions: expected suffix {desc}. \
                     Standardized TLM suffixes prevent confusion between ports, exports, and imps during connect_phase."
                ),
                c.loc(t),
            );
        }
    }
}
