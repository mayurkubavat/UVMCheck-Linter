//! SV-RND-026 (UnrelatedSolveBefore): Flags 'solve a before b' when no
//! constraint of the class (or its base classes) ties a to b, directly or
//! through other variables.

use crate::support::class_items::subtree;
use crate::support::constraints::{constraint_blocks, user_model_classes};
use crate::support::type_text::base_name;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::{HashMap, HashSet};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

/// Union-find over variable names.
#[derive(Default)]
struct Relations {
    parent: HashMap<String, String>,
}

impl Relations {
    fn find(&mut self, s: &str) -> String {
        let p = self.parent.entry(s.to_string()).or_insert_with(|| s.to_string()).clone();
        if p == s {
            return p;
        }
        let r = self.find(&p);
        self.parent.insert(s.to_string(), r.clone());
        r
    }

    fn relate(&mut self, group: &HashSet<String>) {
        let mut first: Option<String> = None;
        for g in group {
            match &first {
                None => {
                    self.find(g);
                    first = Some(g.clone());
                }
                Some(f) => {
                    let (a, b) = (self.find(g), self.find(f));
                    self.parent.insert(a, b);
                }
            }
        }
    }

    fn related(&mut self, a: &str, b: &str) -> bool {
        self.parent.contains_key(a) && self.parent.contains_key(b) && self.find(a) == self.find(b)
    }
}

/// Every name a constraint item reads (swe::referencedSymbols): the names
/// of its expressions and the members they access, guards included.
fn item_names(c: &Cst, item: NodeId) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut loop_vars = HashSet::new();
    for x in subtree(c, item) {
        match c.kind(x) {
            NameExpr => {
                out.insert(c.tree.bare_text(x));
            }
            MemberExpr => {
                let base = c.tree.children(x).next().map(|b| c.tree.bare_text(b));
                let m = c.last_text(x).to_string();
                // `this.x` names x itself.
                if base.as_deref() == Some("this") {
                    out.insert(m);
                } else {
                    out.insert(format!(".{m}"));
                }
            }
            ConstraintItem if c.toks(x).next().is_some_and(|t| c.text_of(t) == "foreach") => {
                // The array a foreach walks; its loop variables are symbols
                // of their own, which relate nothing outside the item.
                let toks: Vec<u32> = c.toks(x).collect();
                let mut depth = 0;
                for &t in &toks {
                    match c.text_of(t) {
                        "[" => depth += 1,
                        "]" => depth -= 1,
                        v if depth == 1 && c.tree.tokens[t as usize].kind == svlang::syntax::TokenKind::Identifier => {
                            loop_vars.insert(v.to_string());
                        }
                        _ => {}
                    }
                }
                if let Some(&t) = toks.get(2) {
                    let name = c.text_of(t);
                    if name == "this" {
                        if let Some(&m) = toks.get(4) {
                            out.insert(c.text_of(m).to_string());
                        }
                    } else {
                        out.insert(name.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    out.retain(|n| !loop_vars.contains(n));
    out
}

/// Entries of a solve-before list: (first token, root name).
type SolveList = Vec<(u32, String)>;

/// A solve-before item's two lists.
fn solve_lists(c: &Cst, item: NodeId) -> Option<(SolveList, SolveList)> {
    let toks: Vec<u32> = c.toks(item).collect();
    if c.text_of(*toks.first()?) != "solve" {
        return None;
    }
    let mut lists = [Vec::new(), Vec::new()];
    let (mut which, mut depth) = (0, 0i32);
    let mut entry: Vec<u32> = Vec::new();
    let flush = |lists: &mut [SolveList; 2], which: usize, entry: &mut Vec<u32>| {
        // swe::rootSymbol: through selects and member accesses, not calls.
        let mut d = 0;
        let call = entry.iter().any(|&t| {
            match c.text_of(t) {
                "[" => d += 1,
                "]" => d -= 1,
                "(" if d == 0 => return true,
                _ => {}
            }
            false
        });
        if let (Some(&first), false) = (entry.first(), call) {
            lists[which].push((first, c.text_of(first).to_string()));
        }
        entry.clear();
    };
    for &t in &toks[1..] {
        let x = c.text_of(t);
        match x {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => depth -= 1,
            _ => {}
        }
        if depth == 0 && matches!(x, "before" | "," | ";") {
            flush(&mut lists, which, &mut entry);
            if x == "before" {
                which = 1;
            }
        } else {
            entry.push(t);
        }
    }
    let [a, b] = lists;
    Some((a, b))
}

fn relate_block(c: &Cst, block: NodeId, rel: &mut Relations) {
    for item in c.tree.children(block).filter(|&x| c.kind(x) == ConstraintItem) {
        if c.toks(item).next().is_some_and(|t| c.text_of(t) == "solve") {
            continue;
        }
        rel.relate(&item_names(c, item));
    }
}

pub(crate) fn unrelated_solve_before(c: &Cst, s: &mut Sink) {
    let classes = user_model_classes(c);
    let by_name: HashMap<String, NodeId> = classes.iter().map(|(k, n)| (n.clone(), *k)).collect();
    for (cls, _) in &classes {
        let mut rel = Relations::default();
        let mut cur = Some(*cls);
        let mut guard = 0;
        while let Some(k) = cur {
            for b in constraint_blocks(c, k) {
                relate_block(c, b, &mut rel);
            }
            guard += 1;
            cur = if guard < 64 { base_name(c, k).and_then(|b| by_name.get(&b).copied()) } else { None };
        }
        for block in constraint_blocks(c, *cls) {
            for item in subtree(c, block).into_iter().filter(|&x| c.kind(x) == ConstraintItem) {
                let Some((solve, after)) = solve_lists(c, item) else { continue };
                for (at, a) in &solve {
                    for (_, b) in &after {
                        if a == b || rel.related(a, b) {
                            continue;
                        }
                        s.add(
                            "SV-RND-026",
                            "UnrelatedSolveBefore",
                            Severity::Warning,
                            format!(
                                "'solve {a} before {b}': no constraint relates these variables, so the order changes \
                                 nothing. Remove it, or add the constraint it was meant to shape."
                            ),
                            c.loc(*at),
                        );
                    }
                }
            }
        }
    }
}
