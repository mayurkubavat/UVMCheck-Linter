//! SV-DUP-002 (CloneFragments): Finds runs of three or more statements (at
//! least 50 tokens) that appear in more than one place, also when variables and
//! literals were renamed consistently.

use crate::support::routine_bodies::is_stmt;
use crate::support::written_tokens::{in_define, tok_where, token_key, written_tokens, Generated, NormToken};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::{Design, Loc};
use std::collections::{HashMap, HashSet};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

/// `file:line` of where a node starts (where its macro was used), as the
/// C++ `where()` prints it.
pub(crate) fn node_where(c: &Cst, n: NodeId) -> String {
    c.first_tok(n).map(|t| tok_where(c, t)).unwrap_or_default()
}

struct Stmt {
    toks: Vec<NormToken>,
    at: String,
    /// token_key of this statement alone, with renaming.
    shape: String,
    loc: Loc,
}

struct StmtList {
    routine: usize,
    stmts: Vec<Stmt>,
    whole_body: bool,
}

/// A user routine's name as the C++ prints it: `class::name` for methods.
fn routine_name(c: &Cst, n: NodeId) -> String {
    let name = c.tree.child(n, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default();
    let mut p = c.parent(n);
    while let Some(x) = p {
        if c.kind(x) == ClassDecl {
            let k = c.tree.child(x, Name).map(|y| c.tree.bare_text(y)).unwrap_or_default();
            return format!("{k}::{name}");
        }
        p = c.parent(x);
    }
    name
}

/// The statement lists (two or more statements) of a routine body, as
/// slang's StatementList: a body or block's statements.
fn statement_lists(c: &Cst, routine: usize, body: NodeId, home: u32, out: &mut Vec<StmtList>) {
    for n in c.subtree(body) {
        if !matches!(c.kind(n), Body | SeqBlock | ParBlock) {
            continue;
        }
        let mut items: Vec<NodeId> = Vec::new();
        for x in c.items(n) {
            // slang's VariableDeclStatements carry no syntax, so the rule
            // skips declarations.
            if c.kind(x) != DataDecl && is_stmt(c, x) {
                items.push(x);
            }
        }
        // A list that a macro expanded to was written once, in the macro.
        let written_here = items.iter().any(|&x| {
            c.first_tok(x).is_some_and(|t| {
                let tok = &c.tree.tokens[t as usize];
                let w = if in_define(c, tok) { tok.written } else { tok.origin };
                w.file == home
            })
        });
        if !written_here {
            continue;
        }
        let whole_body = n == body || (c.parent(n) == Some(body) && c.items(body).len() == 1 && c.kind(n) == SeqBlock);
        let mut l = StmtList { routine, stmts: Vec::new(), whole_body };
        for x in items {
            let mut toks = Vec::new();
            written_tokens(c, x, &mut toks);
            if toks.is_empty() {
                continue;
            }
            let shape = token_key(&toks, true);
            l.stmts.push(Stmt { toks, shape, at: node_where(c, x), loc: c.loc_of(x) });
        }
        if l.stmts.len() >= 2 {
            out.push(l);
        }
    }
}

fn window_tokens(l: &StmtList, first: usize, last: usize) -> Vec<NormToken> {
    l.stmts[first..=last].iter().flat_map(|s| s.toks.iter().cloned()).collect()
}

fn window_key(l: &StmtList, first: usize, last: usize, rename: bool) -> String {
    token_key(&window_tokens(l, first, last), rename)
}

/// The window repeats a short cycle of statement shapes, like an unrolled
/// loop: it matches itself shifted by one cycle.
fn periodic(l: &StmtList, first: usize, last: usize) -> bool {
    let len = last - first + 1;
    (1..=len / 2).any(|p| (first..).take_while(|k| k + p <= last).all(|k| l.stmts[k].shape == l.stmts[k + p].shape))
}

#[derive(Clone, Copy, PartialEq)]
struct Window {
    list: usize,
    first: usize,
    last: usize,
}

pub(crate) fn clone_fragments(d: &Design, csts: &[Cst], out: &mut Sink) {
    // Option `min_tokens` (rules.SV-DUP-002.options).
    fn min_tokens() -> usize {
        crate::options::get("SV-DUP-002", "min_tokens", 50) as usize
    }
    // Option `min_statements` (rules.SV-DUP-002.options).
    fn min_stmts() -> usize {
        crate::options::get("SV-DUP-002", "min_statements", 3) as usize
    }
    let generated = Generated::new(d, csts);
    let mut names: Vec<String> = Vec::new();
    let mut lists: Vec<StmtList> = Vec::new();
    let mut seen = HashSet::new();
    for c in csts {
        for n in c.nodes_of(&[FunctionDecl, TaskDecl]) {
            let Some(body) = c.tree.child(n, Body) else { continue };
            let Some((_, nt)) = c.tree.child(n, Name).and_then(|x| c.tree.token_bounds(x)) else { continue };
            let tok = &c.tree.tokens[nt as usize];
            if tok.from_macro {
                continue;
            }
            let path = c.tree.files[tok.origin.file as usize].to_string_lossy().into_owned();
            let base = path.rsplit(['/', '\\']).next().unwrap_or(&path);
            if base.starts_with("uvm_") || generated.at_path(&path) || !seen.insert((path.clone(), tok.origin.offset)) {
                continue;
            }
            names.push(routine_name(c, n));
            statement_lists(c, names.len() - 1, body, tok.origin.file, &mut lists);
        }
    }

    // Smallest window starting at each statement that reaches both minimums.
    let mut by_key: HashMap<String, Vec<Window>> = HashMap::new();
    let mut windows: Vec<(Window, String)> = Vec::new();
    for (li, l) in lists.iter().enumerate() {
        for i in 0..l.stmts.len() {
            let mut tokens = 0;
            let mut j = i;
            while j < l.stmts.len() {
                tokens += l.stmts[j].toks.len();
                if tokens >= min_tokens() && j - i + 1 >= min_stmts() {
                    break;
                }
                j += 1;
            }
            if j == l.stmts.len() {
                break;
            }
            if periodic(l, i, j) {
                continue;
            }
            let key = window_key(l, i, j, true);
            let w = Window { list: li, first: i, last: j };
            by_key.entry(key.clone()).or_default().push(w);
            windows.push((w, key));
        }
    }

    let overlaps = |a: &Window, b: &Window| {
        lists[a.list].routine == lists[b.list].routine && a.list == b.list && !(a.last < b.first || b.last < a.first)
    };
    let mut reported: HashSet<(usize, usize)> = HashSet::new();
    let mut covered_until: HashMap<usize, usize> = HashMap::new();
    for (w, key) in &windows {
        if covered_until.get(&w.list).is_some_and(|&u| w.first <= u) {
            continue;
        }
        let mut orig = None;
        for o in &by_key[key] {
            if o.list == w.list && o.first == w.first {
                break; // only earlier occurrences
            }
            if !overlaps(o, w) {
                orig = Some(*o);
                break;
            }
        }
        let Some(orig) = orig else { continue };
        let (lo, lw) = (&lists[orig.list], &lists[w.list]);
        let pair = (lo.routine, lw.routine);
        if reported.contains(&pair) {
            continue;
        }
        // Grow the match while both sides go on the same way.
        let (mut a, mut b) = (orig, *w);
        while a.last + 1 < lo.stmts.len()
            && b.last + 1 < lw.stmts.len()
            && !(a.list == b.list && a.last + 1 >= b.first)
            && window_key(lo, a.first, a.last + 1, true) == window_key(lw, b.first, b.last + 1, true)
        {
            a.last += 1;
            b.last += 1;
        }
        let whole = lo.whole_body
            && lw.whole_body
            && a.first == 0
            && b.first == 0
            && a.last + 1 == lo.stmts.len()
            && b.last + 1 == lw.stmts.len();
        if whole && window_key(lo, a.first, a.last, false) == window_key(lw, b.first, b.last, false) {
            continue;
        }
        reported.insert(pair);
        covered_until.insert(b.list, b.last);
        let tokens = window_tokens(lw, b.first, b.last).len();
        out.add(
            "SV-DUP-002",
            "CloneFragments",
            Severity::Warning,
            format!(
                "{} statements ({tokens} tokens) in '{}' repeat code in '{}' ({}), apart from names and literals. A fix in one copy will \
be missed in the other; move the shared steps into a helper routine or a common base class.",
                b.last - b.first + 1,
                names[lw.routine],
                names[lo.routine],
                lo.stmts[a.first].at
            ),
            lw.stmts[b.first].loc,
        );
    }
}
