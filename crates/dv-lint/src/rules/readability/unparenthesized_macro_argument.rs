//! SV-MAC-002 (UnparenthesizedMacroArgument): Flags expression macros that use
//! an argument next to an operator without parentheses, or whose body is not
//! wrapped in parentheses. `define DOUBLE(x) x*2 turns DOUBLE(a+1) into a+1*2;
//! write `define DOUBLE(x) ((x)*2).

use std::collections::{HashMap, HashSet};
use dv_model::Loc;
use sv_lexer::Kind;
use sv_syntax::Origin;
use crate::syntax::Cst;
use crate::{Severity, Sink};

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

pub(crate) fn unparenthesized_macro(c: &Cst, s: &mut Sink) {
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
