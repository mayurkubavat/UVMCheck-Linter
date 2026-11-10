//! What the constraint analyzers read: per user class, its rand integral
//! variables with the values their types allow, and the constraints that
//! bound one variable by constants (`x < 4`, `x inside {[1:3], 7}`,
//! `!(x == 2)`, conjunctions with `&&`, `soft`, `dist`). Anything else
//! (implications, `if`, `foreach`, two variables, function calls) is left
//! out, so what is left is a subset of the real constraints: an empty
//! intersection of a subset is empty for the whole.

use std::collections::HashMap;

use super::values::{const_int, type_range, Set};
use crate::support::ast::is_library_class;
use crate::support::defines_packages::{user_tok, Files};
use crate::syntax::Cst;
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

/// One bound on one variable.
pub(crate) struct Atom {
    pub var: String,
    pub set: Set,
    /// The bound as written.
    pub text: String,
    /// First token, where findings about it go.
    pub tok: u32,
    /// The constraint block it is in (its name).
    pub block: String,
    pub soft: bool,
}

/// A `dist` on one variable: its items as (set, text, first token).
pub(crate) struct Dist {
    pub var: String,
    pub items: Vec<(Set, String, u32)>,
    pub block: String,
}

pub(crate) struct ClassConstraints {
    pub name: String,
    /// Rand integral variables: the values their types allow, and the type text.
    pub vars: HashMap<String, (Set, String)>,
    pub atoms: Vec<Atom>,
    pub dists: Vec<Dist>,
}

/// Every user class of the tree that has rand integral variables and
/// constraints on them.
pub(crate) fn classes(c: &Cst) -> Vec<ClassConstraints> {
    let fs = Files::new(c);
    let mut out = Vec::new();
    for k in c.nodes(ClassDecl) {
        if is_library_class(c, k) {
            continue;
        }
        let Some(nm) = c.tree.child(k, Name) else { continue };
        let Some(nt) = c.first_tok(nm) else { continue };
        if !user_tok(c, &fs, nt) {
            continue;
        }
        let mut cc =
            ClassConstraints { name: c.tree.bare_text(nm), vars: HashMap::new(), atoms: Vec::new(), dists: Vec::new() };
        for item in c.tree.children(k) {
            if c.kind(item) != DataDecl || !c.toks(item).any(|t| matches!(c.text_of(t), "rand" | "randc")) {
                continue;
            }
            let Some(dt) = c.tree.child(item, DataType) else { continue };
            let Some((range, _)) = type_range(c, dt) else { continue };
            let ty = c.trimmed(dt);
            for d in c.tree.children(item).filter(|&d| c.kind(d) == Declarator) {
                // Unpacked dimensions make it an array: not one value.
                if c.tree.children(d).any(|x| c.kind(x) == Dim) {
                    continue;
                }
                if let Some(n) = c.tree.child(d, Name) {
                    cc.vars.insert(c.tree.bare_text(n), (range.clone(), ty.clone()));
                }
            }
        }
        if cc.vars.is_empty() {
            continue;
        }
        for cd in c.tree.children(k).filter(|&n| c.kind(n) == ConstraintDecl) {
            // `static` and `extern`/`pure` prototypes are left out with no body.
            let block = c.tree.child(cd, Name).map(|n| c.tree.bare_text(n)).unwrap_or_default();
            let Some(body) = c.tree.child(cd, ConstraintBlock) else { continue };
            for it in c.tree.children(body).filter(|&n| c.kind(n) == ConstraintItem) {
                let own: Vec<&str> = c.toks(it).map(|t| c.text_of(t)).collect();
                if own.iter().any(|t| matches!(*t, "if" | "foreach" | "unique" | "solve" | "disable")) {
                    continue;
                }
                let soft = own.contains(&"soft");
                let Some(e) = c.tree.children(it).next() else { continue };
                if c.kind(e) == ExprOrDist {
                    if let Some(d) = dist(c, e, &cc.vars, &block) {
                        cc.dists.push(d);
                    }
                    continue;
                }
                let mut conj = Vec::new();
                and_terms(c, e, &mut conj);
                for t in conj {
                    if let Some((var, set)) = atom(c, t, &cc.vars) {
                        let tok = c.first_tok(t).unwrap_or(0);
                        cc.atoms.push(Atom { var, set, text: c.trimmed(t), tok, block: block.clone(), soft });
                    }
                }
            }
        }
        if !cc.atoms.is_empty() || !cc.dists.is_empty() {
            out.push(cc);
        }
    }
    out
}

fn and_terms(c: &Cst, n: NodeId, out: &mut Vec<NodeId>) {
    if c.kind(n) == BinaryExpr && c.toks(n).next().map(|t| c.text_of(t)) == Some("&&") {
        for k in c.tree.children(n) {
            and_terms(c, k, out);
        }
    } else if c.kind(n) == ParenExpr {
        match c.tree.children(n).next() {
            Some(k) => and_terms(c, k, out),
            None => out.push(n),
        }
    } else {
        out.push(n);
    }
}

fn var_of(c: &Cst, n: NodeId, vars: &HashMap<String, (Set, String)>) -> Option<String> {
    if c.kind(n) != NameExpr {
        return None;
    }
    let name = c.tree.bare_text(n);
    vars.contains_key(&name).then_some(name)
}

/// The values a range-list element allows: `7`, `[1:3]`.
fn element(c: &Cst, n: NodeId) -> Option<Set> {
    if c.kind(n) == OpenRange {
        let k: Vec<NodeId> = c.tree.children(n).collect();
        let [lo, hi] = k[..] else { return None };
        return Some(Set::range(const_int(c, lo)?, const_int(c, hi)?));
    }
    const_int(c, n).map(Set::point)
}

/// One bound: (variable, the values it allows).
fn atom(c: &Cst, n: NodeId, vars: &HashMap<String, (Set, String)>) -> Option<(String, Set)> {
    match c.kind(n) {
        ParenExpr => atom(c, c.tree.children(n).next()?, vars),
        PrefixExpr if c.toks(n).next().map(|t| c.text_of(t)) == Some("!") => {
            let (v, s) = atom(c, c.tree.children(n).next()?, vars)?;
            Some((v, s.complement()))
        }
        InsideExpr => {
            let k: Vec<NodeId> = c.tree.children(n).collect();
            let [x, list] = k[..] else { return None };
            let v = var_of(c, x, vars)?;
            if c.kind(list) != RangeList {
                return None;
            }
            let mut set = Set::empty();
            for e in c.tree.children(list) {
                set = set.union(&element(c, e)?);
            }
            Some((v, set))
        }
        BinaryExpr => {
            let op = c.toks(n).next().map(|t| c.text_of(t))?;
            let k: Vec<NodeId> = c.tree.children(n).collect();
            let [l, r] = k[..] else { return None };
            let (v, k, op) = if let Some(v) = var_of(c, l, vars) {
                (v, const_int(c, r)?, op)
            } else {
                let v = var_of(c, r, vars)?;
                let mirrored = match op {
                    "<" => ">",
                    "<=" => ">=",
                    ">" => "<",
                    ">=" => "<=",
                    o => o,
                };
                (v, const_int(c, l)?, mirrored)
            };
            let set = match op {
                "==" => Set::point(k),
                "!=" => Set::point(k).complement(),
                "<" => Set::range(i128::MIN, k - 1),
                "<=" => Set::range(i128::MIN, k),
                ">" => Set::range(k + 1, i128::MAX),
                ">=" => Set::range(k, i128::MAX),
                _ => return None,
            };
            Some((v, set))
        }
        _ => None,
    }
}

fn dist(c: &Cst, n: NodeId, vars: &HashMap<String, (Set, String)>, block: &str) -> Option<Dist> {
    let k: Vec<NodeId> = c.tree.children(n).collect();
    let [x, list] = k[..] else { return None };
    let var = var_of(c, x, vars)?;
    let mut items = Vec::new();
    for di in c.tree.children(list) {
        if c.kind(di) != DistItem {
            return None;
        }
        // `default :/ w` (1800-2023) covers whatever the others leave.
        let e = c.tree.children(di).next()?;
        let set = element(c, e)?;
        items.push((set, c.trimmed(di), c.first_tok(di)?));
    }
    Some(Dist { var, items, block: block.to_string() })
}

/// True when user code turns constraint blocks off and on
/// (`constraint_mode`): then blocks need not all hold at once.
pub(crate) fn uses_constraint_mode(csts: &[Cst]) -> bool {
    csts.iter().any(|c| {
        let fs = Files::new(c);
        c.tree.tokens.iter().enumerate().any(|(i, t)| &*t.text == "constraint_mode" && user_tok(c, &fs, i as u32))
    })
}

/// The values the hard bounds of `var` allow, starting from its type's
/// range; with `block`, only that block's bounds.
pub(crate) fn hard_set(cc: &ClassConstraints, var: &str, block: Option<&str>) -> Set {
    let mut s = cc.vars[var].0.clone();
    for a in cc.atoms.iter().filter(|a| a.var == var && !a.soft && block.is_none_or(|b| a.block == b)) {
        s = s.intersect(&a.set);
    }
    s
}
