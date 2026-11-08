//! Readability rules that read only the source and syntax (builder slice
//! N6.3, group F): dense inline properties, comments that repeat the code,
//! commented-out code and unparenthesized expression macros. Ported from
//! `cpp/src/rules/readability/`; messages are verbatim.

use std::collections::{BTreeSet, HashMap, HashSet};

use dv_model::Loc;
use sv_lexer::Kind;
use sv_syntax::Origin;

use crate::syntax::Cst;
use crate::Check::Syntax;
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-SVA-006", name: "ComplexInlineProperty", check: Syntax(complex_inline_property) },
        Rule { id: "SV-RDB-009", name: "RedundantComment", check: Syntax(redundant_comment) },
        Rule { id: "SV-RDB-003", name: "CommentedOutCode", check: Syntax(commented_out_code) },
        Rule { id: "SV-MAC-002", name: "UnparenthesizedMacroArgument", check: Syntax(unparenthesized_macro) },
    ]
}

fn lower(s: &str) -> String {
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
fn is_library_tok(c: &Cst, t: u32) -> bool {
    let tok = &c.tree.tokens[t as usize];
    if tok.from_macro {
        return true;
    }
    library_file(c, tok.origin.file)
}

fn library_file(c: &Cst, f: u32) -> bool {
    c.tree.files[f as usize]
        .file_name()
        .is_some_and(|n| n.to_string_lossy().starts_with("uvm_"))
}

// ---- SV-SVA-006 ----

fn complex_inline_property(c: &Cst, s: &mut Sink) {
    const LIMIT: usize = 3;
    const TEMPORAL: [&str; 15] = [
        "##", "|->", "|=>", "throughout", "within", "intersect", "until", "s_until", "until_with", "s_until_with",
        "implies", "#-#", "#=#", "nexttime", "s_nexttime",
    ];
    for n in c.nodes(ConcurrentAssert) {
        let Some(first) = c.first_tok(n) else { continue };
        if c.toks(n).next().is_some_and(|k| c.text_of(k) == "expect") {
            continue;
        }
        if is_library_tok(c, first) {
            continue;
        }
        let Some(spec) = c.tree.child(n, PropertyExpr) else { continue };
        let ops = c.tree.token_range(spec).iter().filter(|&&t| TEMPORAL.contains(&c.text_of(t))).count();
        if ops > LIMIT {
            s.add(
                "SV-SVA-006",
                "ComplexInlineProperty",
                Severity::Info,
                format!(
                    "This assertion writes {ops} temporal operators inline. Name the pieces as sequence/property \
                     declarations (s_addr_phase, s_data_phase) so the property reads like the protocol's timing diagram."
                ),
                c.loc(first),
            );
        }
    }
}

// ---- trivia as slang splits it ----

#[derive(Clone, Copy, PartialEq)]
enum Triv {
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
fn split_trivia(text: &str, base: u32) -> Vec<Triv> {
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

// ---- SV-RDB-009 ----

const STOP: [&str; 16] = [
    "the", "a", "an", "to", "of", "and", "for", "in", "on", "with", "this", "that", "is", "it", "from", "into",
];

fn redundant_comment(c: &Cst, s: &mut Sink) {
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
            code.iter().any(|c| c == w || (c.len() >= 3 && w.len() >= 3 && (c.starts_with(w.as_str()) || w.starts_with(c.as_str()))))
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

// ---- SV-RDB-003 ----

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
    let wrappers = [
        ("module m__; initial begin\n", "\nend\nendmodule\n"),
        ("class c__;\n", "\nendclass\n"),
    ];
    wrappers.iter().any(|(a, b)| {
        let tree = sv_syntax::parse_source(&format!("{a}{text}{b}"), "", Default::default());
        tree.errors.is_empty() && tree.pp_diagnostics.is_empty()
    })
}

fn trim_comment(s: &str) -> &str {
    s.trim_start_matches([' ', '\t']).trim_end_matches([' ', '\t', '\r', '\n'])
}

fn commented_out_code(c: &Cst, s: &mut Sink) {
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

// ---- SV-MAC-002 ----

/// A `define in the tree's text, as the preprocessor read it (not in a
/// branch not taken): its name's position, formal argument names (None for
/// a macro without arguments) and body tokens (kind, text).
struct Define {
    file: u32,
    name_at: u32,
    name: String,
    formals: Option<Vec<String>>,
    body: Vec<(Kind, String)>,
}

fn defines(c: &Cst) -> Vec<Define> {
    let t = c.tree;
    let mut out = Vec::new();
    let mut seen_files: HashSet<&std::path::Path> = HashSet::new();
    for f in 0..t.sources.len() as u32 {
        // A file read twice (an include guard skips the second time) defines
        // its macros once.
        if !seen_files.insert(t.files[f as usize].as_path()) && !t.tokens.iter().any(|k| k.origin.file == f) {
            continue;
        }
        let src = &t.sources[f as usize];
        let toks = sv_lexer::lex(src);
        let skipped = |p: u32| t.skipped.iter().any(|&(sf, a, e)| sf == f && a <= p && p < e);
        let mut i = 0;
        while i < toks.len() {
            let tk = &toks[i];
            let text = &src[tk.start as usize..(tk.start + tk.len) as usize];
            if tk.kind != Kind::Directive || text != "`define" || skipped(tk.start) {
                i += 1;
                continue;
            }
            // The name.
            let mut j = i + 1;
            while j < toks.len() && toks[j].kind == Kind::Whitespace {
                j += 1;
            }
            let Some(nt) = toks.get(j) else { break };
            if !matches!(nt.kind, Kind::Identifier | Kind::Keyword | Kind::EscapedIdentifier) {
                i = j;
                continue;
            }
            let name = src[nt.start as usize..(nt.start + nt.len) as usize].to_string();
            j += 1;
            // Formal arguments: '(' right after the name.
            let mut formals = None;
            let at = |k: usize| toks.get(k).map(|x| &src[x.start as usize..(x.start + x.len) as usize]);
            if at(j) == Some("(") {
                let mut names = Vec::new();
                let mut depth = 0;
                let mut expect_name = true;
                while j < toks.len() && !(toks[j].kind == Kind::Newline && toks[j - 1].kind != Kind::LineContinuation) {
                    let s = at(j).unwrap_or("");
                    match s {
                        "(" | "[" | "{" => {
                            depth += 1;
                            if s == "(" && depth == 1 {
                                expect_name = true;
                            }
                        }
                        ")" | "]" | "}" => {
                            depth -= 1;
                            if depth == 0 {
                                j += 1;
                                break;
                            }
                        }
                        "," if depth == 1 => expect_name = true,
                        "=" if depth == 1 => expect_name = false,
                        _ if depth == 1 && expect_name && toks[j].kind == Kind::Identifier => {
                            names.push(s.to_string());
                            expect_name = false;
                        }
                        _ => {}
                    }
                    j += 1;
                }
                formals = Some(names);
            }
            // The body, to the end of the line (continuations join lines).
            let mut body = Vec::new();
            while j < toks.len() && !(toks[j].kind == Kind::Newline && toks[j - 1].kind != Kind::LineContinuation) {
                let k = toks[j].kind;
                // slang keeps a line continuation in a macro body as a token.
                if !k.is_trivia() || k == Kind::LineContinuation {
                    body.push((k, at(j).unwrap_or("").to_string()));
                }
                j += 1;
            }
            out.push(Define { file: f, name_at: nt.start, name, formals, body });
            i = j;
        }
    }
    // The macro table at the end: the last definition of each name.
    let mut last: HashMap<String, usize> = HashMap::new();
    for (i, d) in out.iter().enumerate() {
        last.insert(d.name.clone(), i);
    }
    let keep: HashSet<usize> = last.into_values().collect();
    out.into_iter().enumerate().filter(|(i, _)| keep.contains(i)).map(|(_, d)| d).collect()
}

const OPERATORS: [&str; 27] = [
    "+", "-", "*", "/", "%", "**", "<<", ">>", "<<<", ">>>", "&", "|", "^", "&&", "||", "==", "!=", "===", "!==",
    "<", "<=", ">", ">=", "?", "~", "!", "",
];

fn is_operator(k: Kind, s: &str) -> bool {
    k == Kind::Punct && !s.is_empty() && OPERATORS.contains(&s)
}

fn is_binary_operator(k: Kind, s: &str) -> bool {
    is_operator(k, s) && s != "~" && s != "!"
}

fn wrapped_in_parens(body: &[(Kind, String)]) -> bool {
    if body.len() < 2 || body[0].1 != "(" || body[body.len() - 1].1 != ")" {
        return false;
    }
    let mut depth = 0;
    for (i, (_, s)) in body.iter().enumerate() {
        if s == "(" {
            depth += 1;
        } else if s == ")" {
            depth -= 1;
            if depth == 0 && i + 1 != body.len() {
                return false;
            }
        }
    }
    true
}

fn macro_problems(d: &Define) -> String {
    let Some(formals) = &d.formals else { return String::new() };
    let mut has_op = false;
    for (k, s) in &d.body {
        if s == ";"
            || matches!(k, Kind::Keyword | Kind::MacroPaste | Kind::MacroQuote | Kind::Directive)
            || s.starts_with("`\"\"\"")
        {
            return String::new();
        }
        has_op |= is_operator(*k, s);
    }
    if !has_op {
        return String::new();
    }
    let mut bare: Vec<&str> = Vec::new();
    for (i, (k, s)) in d.body.iter().enumerate() {
        if *k != Kind::Identifier || !formals.iter().any(|f| f == s) {
            continue;
        }
        let before = i > 0 && is_operator(d.body[i - 1].0, &d.body[i - 1].1);
        let after = d.body.get(i + 1).is_some_and(|(k, s)| is_binary_operator(*k, s));
        if (before || after) && !bare.contains(&s.as_str()) {
            bare.push(s);
        }
    }
    let mut out = String::new();
    if !bare.is_empty() {
        out.push_str("argument");
        out.push_str(if bare.len() > 1 { "s " } else { " " });
        for (i, b) in bare.iter().enumerate() {
            out.push_str(if i > 0 { ", '" } else { "'" });
            out.push_str(b);
            out.push('\'');
        }
        out.push_str(" used next to an operator without parentheses");
    }
    if !wrapped_in_parens(&d.body) {
        if !out.is_empty() {
            out.push_str("; ");
        }
        out.push_str("body not wrapped in parentheses");
    }
    out
}

fn unparenthesized_macro(c: &Cst, s: &mut Sink) {
    for d in defines(c) {
        if d.name.starts_with("uvm_") || d.name.starts_with("UVM_") {
            continue;
        }
        let p = macro_problems(&d);
        if p.is_empty() {
            continue;
        }
        let loc: Loc = c.loc_at(Origin { file: d.file, offset: d.name_at });
        s.add(
            "SV-MAC-002",
            "UnparenthesizedMacroArgument",
            Severity::Warning,
            format!(
                "Macro '{}': {p}. After expansion, operator precedence can regroup the caller's expression. \
                 Parenthesize each use of an argument and the whole body: `define {}(...) ((a) op (b)).",
                d.name, d.name
            ),
            loc,
        );
    }
}
