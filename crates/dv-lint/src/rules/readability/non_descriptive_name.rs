//! SV-RDB-007 (NonDescriptiveName): Flags class properties, and locals of
//! routines longer than min_lines (default 10), named tmp, temp, val, obj, foo,
//! d, t and the like (with or without a number; locals also data and value).

use crate::support::class_docs::{class_methods, properties, Classes};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::{Design, Loc};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

fn is_generic_name(name: &str, local: bool) -> bool {
    const STEMS: [&str; 13] =
        ["tmp", "temp", "val", "obj", "foo", "bar", "baz", "var", "thing", "stuff", "dummy", "d", "t"];
    let mut n = name.to_ascii_lowercase();
    while n.len() > 1 && n.ends_with(|ch: char| ch.is_ascii_digit()) {
        n.pop();
    }
    STEMS.contains(&n.as_str()) || (local && (n == "data" || n == "value"))
}

/// Line span of a method body: from its first item to its last.
fn body_lines(c: &Cst, routine: NodeId) -> Option<u32> {
    let body = c.tree.child(routine, Body)?;
    let (a, e) = c.tree.token_bounds(body)?;
    let line = |t: u32| {
        let l = c.loc(t);
        l.expansion.map_or(l.line, |(_, x)| x)
    };
    Some(line(e).saturating_sub(line(a)))
}

/// Variables a routine declares (not arguments; for-loop variables left
/// out, their bodies kept).
fn locals(c: &Cst, routine: NodeId) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    let Some(body) = c.tree.child(routine, Body) else { return out };
    let mut stack = vec![body];
    while let Some(n) = stack.pop() {
        if c.kind(n) == DataDecl {
            let in_for_init = c
                .parent(n)
                .is_some_and(|p| c.kind(p) == LoopStmt && c.keyword(p).is_some_and(|k| c.text_of(k) == "for"));
            if !in_for_init {
                for dd in c.tree.children(n).filter(|&x| c.kind(x) == Declarator) {
                    let Some(nm) = c.tree.child(dd, Name) else { continue };
                    let Some(t) = c.first_tok(nm) else { continue };
                    out.push((c.tree.bare_text(nm), t));
                }
            }
            continue;
        }
        let kids: Vec<NodeId> = c.tree.children(n).collect();
        stack.extend(kids.into_iter().rev());
    }
    out
}

pub(crate) fn non_descriptive_name(_: &Design, csts: &[Cst], s: &mut Sink) {
    const MIN_LINES: u32 = 10;
    let classes = Classes::new(csts);
    let report = |s: &mut Sink, name: &str, where_: String, loc: Loc| {
        s.add(
            "SV-RDB-007",
            "NonDescriptiveName",
            Severity::Info,
            format!(
                "'{name}' {where_} says nothing about what it holds. Name it for its meaning (for example \
                 aligned_addr, expected_txn) so logs and waveforms read without the source."
            ),
            loc,
        );
    };
    for (_, k) in classes.user() {
        let c = &csts[k.ix];
        for (p, t) in properties(c, k.node) {
            if is_generic_name(&p, false) {
                report(s, &p, format!("in class '{}'", k.name), c.loc(t));
            }
        }
        for m in class_methods(csts, k) {
            let mc = &csts[m.ix];
            if !body_lines(mc, m.node).is_some_and(|n| n >= MIN_LINES) {
                continue;
            }
            for (v, t) in locals(mc, m.node) {
                if is_generic_name(&v, true) {
                    report(s, &v, format!("in '{}'", m.name), mc.loc(t));
                }
            }
        }
    }
}
