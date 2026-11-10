//! SV-BUG-001 (IdenticalBranches): Flags if/else and case branches whose bodies
//! are token-identical to an earlier branch of the same statement (clang-tidy
//! bugprone-branch-clone, Sonar S1871).

use crate::support::class_index::{count_statements, strongest_report, Level};
use crate::support::sema_facts::ast_ok;
use crate::support::statements::{case_parts, if_parts, in_bad_body, list, report, stmt, St};
use crate::support::sva_cov::user_code_times;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

/// The end offset of the macro use whose backtick is at `at`: past its
/// name and, when one follows, its parenthesized arguments.
fn macro_use_end(src: &str, at: usize) -> usize {
    let b = src.as_bytes();
    let mut i = at + 1;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
        i += 1;
    }
    let mut j = i;
    while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
        j += 1;
    }
    if j >= b.len() || b[j] != b'(' {
        return i;
    }
    let mut depth = 0;
    while j < b.len() {
        match b[j] {
            b'"' => {
                j += 1;
                while j < b.len() && b[j] != b'"' {
                    if b[j] == b'\\' {
                        j += 1;
                    }
                    j += 1;
                }
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return j + 1;
                }
            }
            _ => {}
        }
        j += 1;
    }
    b.len()
}

/// `stripCommentsAndSpace`: comments dropped, whitespace runs one space,
/// string literals verbatim.
fn strip_comments_and_space(src: &str) -> String {
    let b = src.as_bytes();
    let mut out: Vec<u8> = Vec::new();
    let mut space = false;
    let mut i = 0;
    while i < b.len() {
        let ch = b[i];
        if ch == b'"' {
            if space && !out.is_empty() {
                out.push(b' ');
            }
            space = false;
            out.push(ch);
            i += 1;
            while i < b.len() {
                out.push(b[i]);
                if b[i] == b'\\' && i + 1 < b.len() {
                    i += 1;
                    out.push(b[i]);
                    i += 1;
                    continue;
                }
                if b[i] == b'"' {
                    break;
                }
                i += 1;
            }
            i += 1;
            continue;
        }
        if ch == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            space = true;
            i += 1;
            continue;
        }
        if ch == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i += 2;
            space = true;
            continue;
        }
        if ch.is_ascii_whitespace() {
            space = true;
            i += 1;
            continue;
        }
        if space && !out.is_empty() {
            out.push(b' ');
        }
        space = false;
        out.push(ch);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `swe::originalText`: the statement as written in its file, macro uses
/// unexpanded, comments removed and whitespace collapsed.
pub(crate) fn original_text(c: &Cst, n: NodeId) -> String {
    let fallback = || strip_comments_and_space(&c.text(n));
    let Some((a, b)) = c.tree.token_bounds(n) else { return String::new() };
    let ta = &c.tree.tokens[a as usize];
    let tb = &c.tree.tokens[b as usize];
    if ta.origin.file != tb.origin.file {
        return fallback();
    }
    let Some(src) = c.tree.sources.get(ta.origin.file as usize) else { return fallback() };
    let start = ta.origin.offset as usize;
    let end = if tb.from_macro {
        macro_use_end(src, tb.origin.offset as usize)
    } else {
        tb.origin.offset as usize + tb.text.len()
    };
    if end < start || end > src.len() {
        return fallback();
    }
    strip_comments_and_space(&src[start..end])
}

/// Lone return/break/continue statements, empty blocks and report-only
/// branches are commonly repeated on purpose.
fn trivial_branch(c: &Cst, n: NodeId) -> bool {
    let mut s = n;
    while matches!(stmt(c, s), Some(St::Seq | St::Par(_))) {
        let body = list(c, s);
        match body.as_slice() {
            [] => return true,
            [one] => s = *one,
            _ => break,
        }
    }
    if matches!(stmt(c, s), Some(St::Empty | St::Return | St::Break | St::Continue)) {
        return true;
    }
    // A block of several statements counts each twice in slang's walk
    // (measured: two statements with a report are not trivial).
    let k = if matches!(stmt(c, s), Some(St::Seq | St::Par(_))) { 2 } else { 1 };
    k * count_statements(c, s) <= 2 && strongest_report(c, s) != Level::None
}

fn single_macro_call(text: &str) -> bool {
    if !text.starts_with("`uvm_") {
        return false;
    }
    match text.find(';') {
        None => true,
        Some(i) => i + 1 == text.len(),
    }
}

/// The statement slang binds for a branch: an unnamed `begin ... end`
/// holding one statement and no declarations is that statement.
fn bound_stmt(c: &Cst, n: NodeId) -> NodeId {
    let mut s = n;
    while c.kind(s) == SeqBlock && !c.toks(s).any(|t| c.text_of(t) == ":") && c.tree.child(s, StmtLabel).is_none() {
        match list(c, s).as_slice() {
            [one] if stmt(c, *one) != Some(St::Decl) => s = *one,
            _ => break,
        }
    }
    s
}

/// An `if` that is the `else` of another `if` (part of its chain).
fn chained(c: &Cst, n: NodeId) -> bool {
    c.parent(n).filter(|&p| c.kind(p) == ElseClause).and_then(|p| c.parent(p)).is_some_and(|g| c.kind(g) == IfStmt)
}

pub(crate) fn identical_branches(c: &Cst, s: &mut Sink) {
    let mut stmts: Vec<NodeId> = c.nodes(IfStmt);
    stmts.extend(c.nodes(CaseStmt));
    stmts.sort_by_key(|n| n.0);
    for n in stmts {
        let what = match stmt(c, n) {
            Some(St::If) if !chained(c, n) => "if/else",
            Some(St::Case) => "case",
            _ => continue,
        };
        let times = user_code_times(c, n);
        if times == 0 || in_bad_body(c, n) {
            continue;
        }
        let mut branches = Vec::new();
        if what == "case" {
            let p = case_parts(c, n);
            if p.sel.is_some_and(|e| !ast_ok(c, e)) {
                continue;
            }
            branches.extend(p.items.iter().map(|(_, b)| *b));
            branches.extend(p.default);
        } else {
            let mut cur = n;
            let mut ok = true;
            loop {
                let (cond, then, els) = if_parts(c, cur);
                if cond.is_some_and(|e| !ast_ok(c, e)) {
                    ok = false;
                    break;
                }
                branches.extend(then);
                match els {
                    Some(e) if c.kind(e) == IfStmt => cur = e,
                    Some(e) => {
                        branches.push(e);
                        break;
                    }
                    None => break,
                }
            }
            if !ok {
                continue;
            }
        }
        let branches: Vec<NodeId> = branches.into_iter().map(|b| bound_stmt(c, b)).collect();
        let texts: Vec<String> = branches.iter().map(|&b| original_text(c, b)).collect();
        for i in 1..branches.len() {
            if texts[i].is_empty() || trivial_branch(c, branches[i]) || single_macro_call(&texts[i]) {
                continue;
            }
            let Some(j) = (0..i).find(|&j| texts[j] == texts[i]) else { continue };
            let (Some(ti), Some(tj)) = (c.first_tok(branches[i]), c.first_tok(branches[j])) else {
                continue;
            };
            let lj = c.loc(tj);
            let line = lj.expansion.map_or(lj.line, |e| e.1);
            report(
                s,
                times,
                "SV-BUG-001",
                "IdenticalBranches",
                Severity::Warning,
                format!(
                    "This {what} branch is identical to the branch at line {line}. A pasted branch that was never \
                     edited is the usual cause; if both really do the same thing, merge the conditions."
                ),
                c.loc(ti),
            );
        }
    }
}
