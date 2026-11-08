//! SV-DUP-004 (RepeatedStringLiteral): Flags a string literal, such as a report
//! ID or a file name, written four or more times in the code.

use crate::support::class_roles::user_classes;
use crate::support::routine_owner::{bad_body, free_routine_names, routine_class};
use crate::support::written_tokens::{in_define, Generated};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::{Design, Loc};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use sv_lexer::Kind;
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

fn args_of(c: &Cst, n: NodeId) -> Vec<NodeId> {
    let Some(al) = c.tree.child(n, ArgList) else { return Vec::new() };
    c.tree.children(al).filter(|&a| c.kind(a) == Arg).collect()
}

/// The argument whose strings are names, not text: config keys, instance
/// names given to create() and new().
fn name_argument(c: &Cst, n: NodeId) -> Option<NodeId> {
    let args = args_of(c, n);
    if c.kind(n) == NewExpr {
        return args.first().copied();
    }
    let callee = c.tree.children(n).next()?;
    if c.kind(callee) != ScopedExpr {
        return None;
    }
    let method = c.last_text(callee);
    let inner = c.tree.children(callee).next()?;
    match method {
        "create" if c.kind(inner) == ScopedExpr && c.last_text(inner) == "type_id" => args.first().copied(),
        "set" | "get"
            if c.kind(inner) == NameExpr && c.keyword(inner).is_some_and(|t| c.text_of(t) == "uvm_config_db") =>
        {
            args.get(2).copied()
        }
        _ => None,
    }
}

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(ch) = it.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(x) => out.push(x),
            None => {}
        }
    }
    out
}

fn worth_naming(v: &str) -> bool {
    let b = v.as_bytes();
    if b.len() < 4 || b.iter().filter(|c| c.is_ascii_alphabetic()).count() < 3 {
        return false;
    }
    // Pure format strings ("%0d", "%s: %h") say nothing a constant could name.
    let mut stripped = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            while i + 1 < b.len() && !b[i + 1].is_ascii_alphabetic() {
                i += 1;
            }
            i += 2;
            continue;
        }
        if b[i].is_ascii_alphabetic() {
            stripped += 1;
        }
        i += 1;
    }
    stripped >= 3
}

pub(crate) fn repeated_string_literal(d: &Design, csts: &[Cst], out: &mut Sink) {
    // Option `min_count` (rules.SV-DUP-004.options).
    fn min_count() -> usize {
        crate::options::get("SV-DUP-004", "min_count", 4) as usize
    }
    let mut generated = Generated::new(d, csts);
    let mut seen = HashSet::new();
    let mut uses: Vec<(String, Loc)> = Vec::new();
    let free = free_routine_names(csts);
    for c in csts {
        // Routine bodies, and module, interface and program items.
        let mut regions: Vec<NodeId> = Vec::new();
        let mut taken: HashSet<NodeId> = HashSet::new();
        for n in c.nodes_of(&[FunctionDecl, TaskDecl, ModuleDecl, InterfaceDecl, ProgramDecl]) {
            let r = if matches!(c.kind(n), FunctionDecl | TaskDecl) {
                let Some(b) = c.tree.child(n, Body) else { continue };
                if bad_body(d, c, b, routine_class(d, c, n), &free) {
                    continue;
                }
                b
            } else {
                n
            };
            // Skip a region inside one already taken (an ancestor of it).
            let inside = std::iter::successors(Some(r), |&x| c.parent(x)).any(|a| taken.contains(&a));
            if !inside {
                taken.insert(r);
                regions.push(r);
            }
        }
        for r in regions {
            let sub = c.subtree(r);
            let mut skip: HashSet<u32> = HashSet::new();
            for &n in &sub {
                if matches!(c.kind(n), CallExpr | NewExpr) {
                    if let Some(a) = name_argument(c, n) {
                        let e = c.tree.children(a).next().map(|e| c.strip(e));
                        if let Some(e) = e.filter(|&e| c.kind(e) == Literal) {
                            skip.extend(c.tree.token_range(e));
                        }
                    }
                }
            }
            let lits = sub.iter().filter(|&&n| c.kind(n) == Literal).filter_map(|&n| c.first_tok(n).map(|t| (n, t)));
            for (lit, t) in lits {
                let tok = &c.tree.tokens[t as usize];
                // slang's AST does not visit attribute values.
                let in_attr = || std::iter::successors(c.parent(lit), |&x| c.parent(x)).any(|a| c.kind(a) == Attribute);
                if tok.kind != Kind::StringLiteral || skip.contains(&t) || in_define(c, tok) || in_attr() {
                    continue;
                }
                let w = if tok.from_macro { tok.written } else { tok.origin };
                let path =
                    c.tree.files.get(w.file as usize).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
                let base = path.rsplit(['/', '\\']).next().unwrap_or(&path);
                if base.starts_with("uvm_") || !seen.insert((path.clone(), w.offset)) {
                    continue;
                }
                let loc = c.loc_at(w);
                if generated.at(loc) {
                    continue;
                }
                let raw = &tok.text;
                let inner = raw.strip_prefix('"').and_then(|s| s.strip_suffix('"')).unwrap_or(raw);
                uses.push((unescape(inner), loc));
            }
        }
    }
    let class_names: HashSet<&str> = user_classes(d).map(|k| k.name.as_str()).collect();
    let mut by_value: BTreeMap<String, Vec<Loc>> = BTreeMap::new();
    for (v, l) in uses {
        if worth_naming(&v) && !class_names.contains(v.as_str()) {
            by_value.entry(v).or_default().push(l);
        }
    }
    for (value, locs) in by_value {
        if locs.len() < min_count() {
            continue;
        }
        let files: BTreeSet<&str> = locs.iter().map(|l| d.file_name(*l)).collect();
        out.add(
            "SV-DUP-004",
            "RepeatedStringLiteral",
            Severity::Info,
            format!(
                "The string \"{value}\" is written {} times in {} {}. Declare it once as a localparam string or static const in the \
package, so a change or a typo cannot split the copies.",
                locs.len(),
                files.len(),
                if files.len() == 1 { "file" } else { "files" }
            ),
            locs[0],
        );
    }
}
