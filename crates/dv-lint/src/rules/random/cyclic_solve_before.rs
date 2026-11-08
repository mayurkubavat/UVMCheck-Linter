//! SV-RND-010 (CyclicSolveBefore): Detects cyclic dependencies in
//! solve...before constraint ordering (e.g. solve a before b and solve b before
//! a).

use crate::support::class_items::{class_decls, span_text, subtree, trim};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::{HashMap, HashSet};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

/// A solve-before item: its `solve` token and the two lists of expressions
/// (each as written, trimmed, with its tokens).
fn solve_before(c: &Cst, item: NodeId) -> Option<(u32, Vec<String>, Vec<String>)> {
    let toks: Vec<u32> = c.toks(item).collect();
    let first = *toks.first()?;
    if c.text_of(first) != "solve" {
        return None;
    }
    let mut lists: Vec<Vec<String>> = vec![Vec::new(), Vec::new()];
    let mut which = 0;
    let mut start: Option<u32> = None;
    let mut depth = 0i32;
    let flush = |lists: &mut Vec<Vec<String>>, which: usize, start: &mut Option<u32>, end: u32| {
        if let Some(a) = start.take() {
            lists[which].push(trim(&span_text(c, a, end)).to_string());
        }
    };
    let mut prev = first;
    for &t in &toks[1..] {
        let x = c.text_of(t);
        match x {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => depth -= 1,
            _ => {}
        }
        if depth == 0 && (x == "before" || x == "," || x == ";") {
            flush(&mut lists, which, &mut start, prev);
            if x == "before" {
                which = 1;
            }
        } else if start.is_none() {
            start = Some(t);
        }
        prev = t;
    }
    let after = lists.pop()?;
    let before = lists.pop()?;
    Some((first, before, after))
}

pub(crate) fn cyclic_solve_before(c: &Cst, s: &mut Sink) {
    for cls in class_decls(c) {
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        let has_path = |adj: &HashMap<String, Vec<String>>, start: &str, target: &str| -> bool {
            if start == target {
                return true;
            }
            let mut q = std::collections::VecDeque::from([start.to_string()]);
            let mut seen: HashSet<String> = HashSet::from([start.to_string()]);
            while let Some(cur) = q.pop_front() {
                if let Some(next) = adj.get(&cur) {
                    for n in next {
                        if n == target {
                            return true;
                        }
                        if seen.insert(n.clone()) {
                            q.push_back(n.clone());
                        }
                    }
                }
            }
            false
        };
        for n in subtree(c, cls) {
            if c.kind(n) != ConstraintItem {
                continue;
            }
            let Some((kw, before, after)) = solve_before(c, n) else { continue };
            for u in &before {
                for v in &after {
                    if u.is_empty() || v.is_empty() {
                        continue;
                    }
                    if u == v || has_path(&adj, v, u) {
                        s.add(
                            "SV-RND-010",
                            "CyclicSolveBefore",
                            Severity::Error,
                            format!(
                                "Cyclic dependency in solve...before order involving variables '{u}' and '{v}'. Cyclic solve-before constraints form an unsatisfiable dependency graph per IEEE 1800 § 18.5.10."
                            ),
                            c.loc(kw),
                        );
                    } else {
                        adj.entry(u.clone()).or_default().push(v.clone());
                    }
                }
            }
        }
    }
}
