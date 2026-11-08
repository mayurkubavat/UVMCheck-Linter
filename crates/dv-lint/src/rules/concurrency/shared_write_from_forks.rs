//! SV-CNC-004 (SharedWriteFromForks): Flags class properties written in two or
//! more branches of one fork, or in a join_none thread and in its parent,
//! without a semaphore (SpotBugs IS2_INCONSISTENT_SYNC).

use std::collections::{BTreeMap, HashSet};
use sv_syntax::{NodeId, SyntaxKind};
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::procedural::{Sym, add_n, calls_in, is_semaphore_get, items, join_kind, routine_name, start, written};

/// A fork's branches: its statements less declarations and empty ones.
fn branches(c: &Cst, f: NodeId) -> Vec<NodeId> {
    let it = items(c, f);
    if it.len() == 1 {
        return it;
    }
    it.into_iter().filter(|&x| !matches!(c.kind(x), DataDecl | EmptyStmt)).collect()
}

fn uses_semaphore(c: &Cst, n: NodeId) -> bool {
    calls_in(c, n).into_iter().any(|x| is_semaphore_get(c, x))
}

fn property_writes(c: &Cst, n: NodeId) -> Vec<(Sym, String)> {
    written(c, n, false)
        .into_iter()
        .filter(|x| x.2.is_some_and(|d| d.kind == DeclKind::Property))
        .map(|x| (x.0, x.1))
        .collect()
}

/// A fork's statements as slang's block body: the list when there are two
/// or more, else the one statement.
fn fork_body_writes(c: &Cst, f: NodeId) -> Vec<(Sym, String)> {
    let mut out: Vec<(Sym, String)> = Vec::new();
    for it in items(c, f) {
        for w in property_writes(c, it) {
            if !out.iter().any(|x| x.0 == w.0) {
                out.push(w);
            }
        }
    }
    out
}

pub(crate) fn shared_write(c: &Cst, s: &mut Sink) {
    for (r, n) in user_routines(c) {
        let where_ = format!("'{}'", routine_name(c, r));
        let report = |s: &mut Sink, f: NodeId, how: String, name: &str| {
            if let Some(loc) = start(c, f) {
                add_n(
                    s,
                    n,
                    "SV-CNC-004",
                    "SharedWriteFromForks",
                    Severity::Warning,
                    format!(
                        "Property '{name}' {how} in {where_}, with no semaphore around the writes. The final value depends on thread scheduling; give each thread its own variable, or guard the writes with a semaphore."
                    ),
                    loc,
                );
            }
        };
        for f in subtree(c, r).into_iter().filter(|&x| c.kind(x) == ParBlock) {
            // Branches written by two or more.
            let br = branches(c, f);
            if br.len() >= 2 {
                let mut writers: BTreeMap<Sym, (usize, String)> = BTreeMap::new();
                for b in &br {
                    if uses_semaphore(c, *b) {
                        continue;
                    }
                    for (sym, name) in property_writes(c, *b) {
                        writers.entry(sym).or_insert((0, name)).0 += 1;
                    }
                }
                for (_, (count, name)) in writers {
                    if count >= 2 {
                        report(s, f, format!("is written by {count} branches of the same fork"), &name);
                    }
                }
            }
            // A join_none thread and the statements after it.
            if join_kind(c, f) != "join_none" {
                continue;
            }
            let Some(p) = c.parent(f) else { continue };
            if !matches!(c.kind(p), SeqBlock | ParBlock | Body) {
                continue;
            }
            let list = items(c, p);
            if list.len() < 2 {
                continue;
            }
            if uses_semaphore(c, f) {
                continue;
            }
            let mine = fork_body_writes(c, f);
            if mine.is_empty() {
                continue;
            }
            let ix = list.iter().position(|&x| x == f).unwrap_or(list.len());
            let mut after = HashSet::new();
            let mut guarded = false;
            for &j in &list[ix + 1..] {
                if uses_semaphore(c, j) {
                    guarded = true;
                    break;
                }
                for (sym, _) in property_writes(c, j) {
                    after.insert(sym);
                }
            }
            if guarded {
                continue;
            }
            for (sym, name) in mine {
                if after.contains(&sym) {
                    report(s, f, "is written by a join_none thread and by the code that forked it".into(), &name);
                }
            }
        }
    }
}
