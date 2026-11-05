//! Type checks (builder phase N2): what IEEE 1800 rejects about how a
//! declared object is used, found by resolving names in the scopes.
//!
//! - a bit-select of a `real`, or a `real` index (6.12);
//! - an edge (`posedge`, `negedge`, `edge`) of a `real` (6.12);
//! - a default value on a packed struct member (7.2.2);
//! - a procedural assignment to a net (10.3, 14.3);
//! - an assignment used as an expression without parentheses (11.3.6);
//! - `return` with a value in a void function (13.4.1);
//! - `fork ... join` or `join_any` in a function (13.4.4);
//! - `return` inside a fork (9.3.3);
//! - an enum with x/z values on a two-state base, a name after an x/z
//!   one with no value, or a sized value of the wrong width (6.19);
//! - an integer assigned to an enum variable, or an enum variable in a
//!   compound assignment (6.19.3, 6.19.4);
//! - a parameter whose value uses a specparam (6.20.5);
//! - a `randc` variable in `solve ... before`, a soft constraint or a
//!   distribution (18.5.4, 18.5.10, 18.5.14).
//!
//! A name the checks cannot resolve for sure (inherited, imported,
//! hierarchical) never makes an error.

use std::collections::HashMap;

use sv_syntax::{Child, NodeId, SyntaxKind, Tree};
use SyntaxKind::*;

use crate::names::Diag;
use crate::scopes::{Scopes, SymKind, ROOT, UNIT};

fn toks(t: &Tree, n: NodeId) -> Vec<&str> {
    t.node(n)
        .children
        .iter()
        .filter_map(|c| match c {
            Child::Token(i) => Some(&*t.tokens[*i as usize].text),
            Child::Node(_) => None,
        })
        .collect()
}

fn first_tok(t: &Tree, n: NodeId) -> Option<&str> {
    t.token_bounds(n).map(|(f, _)| &*t.tokens[f as usize].text)
}

fn name_of(t: &Tree, n: NodeId) -> String {
    t.child(n, Name).map(|c| t.bare_text(c)).unwrap_or_default()
}

const ASSIGN_OPS: &[&str] = &["=", "<=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=", "<<<=", ">>>="];
const REAL_TYPES: &[&str] = &["real", "shortreal", "realtime"];
const PROCEDURAL: &[&str] = &["initial", "always", "always_ff", "always_comb", "always_latch", "final"];

/// What a name resolved to: its kind and declared data type.
#[derive(Clone, Copy)]
struct Decl {
    kind: SymKind,
    tree: usize,
    dtype: Option<NodeId>,
    /// Declared with unpacked dimensions (an array of the type).
    array: bool,
}

struct Ctx<'a> {
    trees: &'a [Tree],
    scopes: &'a Scopes,
    /// Values by (scope, name).
    values: HashMap<(usize, &'a str), Vec<usize>>,
    /// Scope opened by each declaration node.
    opener: HashMap<(usize, NodeId), usize>,
    /// Typedefs by name, anywhere: whether every one is an enum / a real.
    enums: HashMap<String, bool>,
    reals: HashMap<String, bool>,
    /// Class declarations by name.
    classes: HashMap<String, Vec<(usize, NodeId)>>,
}

pub fn check(trees: &[Tree], scopes: &Scopes) -> Vec<Diag> {
    let mut values: HashMap<(usize, &str), Vec<usize>> = HashMap::new();
    for (i, s) in scopes.symbols.iter().enumerate() {
        if matches!(
            s.kind,
            SymKind::Net
                | SymKind::Variable
                | SymKind::Parameter
                | SymKind::Argument
                | SymKind::EnumValue
                | SymKind::Specparam
                | SymKind::Genvar
        ) {
            values.entry((s.scope, s.name.as_str())).or_default().push(i);
        }
    }
    let mut opener = HashMap::new();
    for (i, sc) in scopes.scopes.iter().enumerate() {
        if let Some(y) = sc.symbol {
            let y = &scopes.symbols[y];
            opener.insert((y.tree, y.node), i);
        }
    }
    let mut classes: HashMap<String, Vec<(usize, NodeId)>> = HashMap::new();
    let mut enums: HashMap<String, bool> = HashMap::new();
    let mut reals: HashMap<String, bool> = HashMap::new();
    for (ix, t) in trees.iter().enumerate() {
        for n in t.preorder() {
            if t.kind(n) == ClassDecl {
                classes.entry(name_of(t, n)).or_default().push((ix, n));
            }
            if t.kind(n) != TypedefDecl {
                continue;
            }
            let name = name_of(t, n);
            let dt = t.child(n, DataType);
            let first = dt.and_then(|d| first_tok(t, d)).unwrap_or("");
            let dims = t.child(n, Dim).is_some();
            *enums.entry(name.clone()).or_insert(true) &= first == "enum" && !dims;
            *reals.entry(name).or_insert(true) &= REAL_TYPES.contains(&first) && !dims;
        }
    }
    let cx = Ctx { trees, scopes, values, opener, enums, reals, classes };
    let mut out = Vec::new();
    for (ix, t) in trees.iter().enumerate() {
        let parent = parents(t);
        for n in t.preorder() {
            cx.node(ix, t, n, &parent, &mut out);
        }
    }
    out
}

/// Width of an integral type written with literal bounds only.
fn literal_width(t: &Tree, dt: NodeId) -> Option<u64> {
    let tk = toks(t, dt);
    let mut w: u64 = match tk.first().copied()? {
        "int" | "integer" => 32,
        "shortint" => 16,
        "longint" => 64,
        "byte" => 8,
        "bit" | "logic" | "reg" => 1,
        _ => return None,
    };
    for d in t.children(dt) {
        if t.kind(d) != Dim {
            return None;
        }
        let (a, b) = literal_range(t, d)?;
        w = w.checked_mul(a.abs_diff(b) + 1)?;
    }
    Some(w)
}

/// `[a:b]` or `[n]` with literal numbers.
fn literal_range(t: &Tree, d: NodeId) -> Option<(i64, i64)> {
    let tk = toks(t, d);
    let nums: Vec<i64> = t
        .children(d)
        .map(|c| (t.kind(c) == Literal).then(|| t.bare_text(c).parse::<i64>().ok()).flatten())
        .collect::<Option<_>>()?;
    match (nums.as_slice(), tk.contains(&":")) {
        ([a, b], true) => Some((*a, *b)),
        ([n], false) if *n > 0 => Some((0, n - 1)),
        _ => None,
    }
}

/// `'{a, b, c}` for an unpacked array with another number of elements
/// (10.10, 5.10: no C-like flattening).
fn array_pattern(t: &Tree, n: NodeId, parent: &[u32], err: &mut impl FnMut(NodeId, String)) {
    let Some(d) = up(parent, n) else { return };
    if t.kind(d) != Declarator {
        return;
    }
    let Some(dim) = t.children(d).find(|&c| t.kind(c) == Dim) else { return };
    let Some((a, b)) = literal_range(t, dim) else { return };
    let own = toks(t, n);
    if own.contains(&":") || own.len() != t.children(n).count() + 1 {
        // Keyed, `default:`, or a replication `'{n{...}}`.
        return;
    }
    let count = t.children(n).count() as u64;
    let want = a.abs_diff(b) + 1;
    if count != want {
        err(n, format!("array '{}' has {want} elements, not {count}", name_of(t, d)));
    }
}

fn parents(t: &Tree) -> Vec<u32> {
    let mut parent = vec![u32::MAX; t.nodes.len()];
    for n in t.preorder() {
        for c in t.children(n) {
            parent[c.0 as usize] = n.0;
        }
    }
    parent
}

fn up(parent: &[u32], n: NodeId) -> Option<NodeId> {
    let p = parent[n.0 as usize];
    (p != u32::MAX).then_some(NodeId(p))
}

impl Ctx<'_> {
    /// A streaming concatenation assigned to a narrower integral target (11.4.14).
    fn stream_into(&self, ix: usize, t: &Tree, n: NodeId, parent: &[u32], err: &mut impl FnMut(NodeId, String)) {
        let Some(p) = up(parent, n) else { return };
        let target = match t.kind(p) {
            Declarator if t.children(p).last() == Some(n) => {
                if t.child(p, Dim).is_some() {
                    return;
                }
                let Some(dd) = up(parent, p) else { return };
                t.child(dd, DataType).and_then(|dt| literal_width(t, dt))
            }
            BinaryExpr if toks(t, p).first() == Some(&"=") && t.children(p).nth(1) == Some(n) => {
                let Some(lhs) = t.children(p).next() else { return };
                self.name_decl(ix, t, lhs, parent)
                    .filter(|d| !d.array)
                    .and_then(|d| d.dtype.and_then(|dt| literal_width(&self.trees[d.tree], dt)))
            }
            _ => None,
        };
        let Some(target) = target else { return };
        // The operands: in the inner `{...}`, after any slice size.
        let mut width: u64 = 0;
        let kids: Vec<NodeId> = t.children(n).collect();
        let ops: Vec<NodeId> = {
            // Children after the second `{`: the node list holds the slice
            // size (if any) first.
            let mut braces = 0;
            let mut out = Vec::new();
            for c in &t.node(n).children {
                match c {
                    Child::Token(i) if &*t.tokens[*i as usize].text == "{" => braces += 1,
                    Child::Node(x) if braces >= 2 => out.push(*x),
                    _ => {}
                }
            }
            out
        };
        let _ = kids;
        for o in ops {
            let w = match t.kind(o) {
                NameExpr => self
                    .name_decl(ix, t, o, parent)
                    .filter(|d| !d.array)
                    .and_then(|d| d.dtype.and_then(|dt| literal_width(&self.trees[d.tree], dt))),
                _ => None,
            };
            let Some(w) = w else { return };
            width += w;
        }
        if width > target {
            err(n, format!("stream of {width} bits is wider than its {target}-bit target"));
        }
    }

    /// The declaration a simple name at `at` refers to.
    fn resolve(&self, ix: usize, t: &Tree, at: NodeId, name: &str, parent: &[u32]) -> Option<Decl> {
        let mut cur = Some(at);
        while let Some(a) = cur {
            if let Some(&s) = self.opener.get(&(ix, a)) {
                return self.in_scopes(ix, s, name);
            }
            if a == t.root {
                return self.in_scopes(ix, UNIT, name);
            }
            // A block's own declarations, or a loop's.
            if !matches!(t.kind(a), ModuleDecl | InterfaceDecl | ProgramDecl | PackageDecl | ClassDecl) {
                for d in t.children(a).filter(|&c| t.kind(c) == DataDecl) {
                    if let Some(x) = t.children(d).find(|&x| t.kind(x) == Declarator && name_of(t, x) == name) {
                        let array = t.child(x, Dim).is_some();
                        return Some(Decl { kind: SymKind::Variable, tree: ix, dtype: t.child(d, DataType), array });
                    }
                }
            }
            cur = up(parent, a);
        }
        None
    }

    fn own(&self, ix: usize, s: usize, name: &str) -> Option<Decl> {
        let ids = self.values.get(&(s, name))?;
        let i = *ids.iter().find(|&&i| s != UNIT || self.scopes.symbols[i].tree == ix)?;
        let y = &self.scopes.symbols[i];
        let t = &self.trees[y.tree];
        let array = y.item.is_some_and(|it| t.kind(it) == Declarator && t.child(it, Dim).is_some());
        Some(Decl { kind: y.kind, tree: y.tree, dtype: y.dtype, array })
    }

    fn in_scopes(&self, ix: usize, s: usize, name: &str) -> Option<Decl> {
        let mut cur = Some(s);
        while let Some(c) = cur {
            if c == ROOT {
                break;
            }
            if let Some(d) = self.own(ix, c, name) {
                return Some(d);
            }
            // A class with a base: the name may be inherited.
            if let Some(y) = self.scopes.scopes[c].symbol {
                let y = &self.scopes.symbols[y];
                if y.kind == SymKind::Class {
                    let t = &self.trees[y.tree];
                    if crate::names::extends(t, y.node).is_some() {
                        return None;
                    }
                }
            }
            cur = self.scopes.scopes[c].parent;
            // Design units and packages see `$unit` of their own file.
            if cur == Some(ROOT) {
                cur = Some(UNIT);
            } else if c == UNIT {
                break;
            }
        }
        None
    }

    fn is_real(&self, d: &Decl) -> bool {
        let t = &self.trees[d.tree];
        let Some(dt) = d.dtype else { return false };
        if d.array {
            return false;
        }
        let text = t.bare_text(dt);
        REAL_TYPES.contains(&text.as_str()) || self.reals.get(&text).copied().unwrap_or(false)
    }

    fn is_enum(&self, d: &Decl) -> bool {
        let t = &self.trees[d.tree];
        let Some(dt) = d.dtype else { return false };
        if first_tok(t, dt) == Some("enum") {
            return true;
        }
        let one = t.token_bounds(dt).is_some_and(|(a, b)| a == b);
        one && self.enums.get(&t.bare_text(dt)).copied().unwrap_or(false)
    }

    fn name_decl(&self, ix: usize, t: &Tree, e: NodeId, parent: &[u32]) -> Option<Decl> {
        if t.kind(e) != NameExpr || t.token_bounds(e).is_none_or_multi() {
            return None;
        }
        self.resolve(ix, t, e, &t.bare_text(e), parent)
    }

    fn node(&self, ix: usize, t: &Tree, n: NodeId, parent: &[u32], out: &mut Vec<Diag>) {
        let mut err = |node: NodeId, message: String| out.push(Diag { tree: ix, node, message });
        match t.kind(n) {
            SelectExpr => {
                let kids: Vec<NodeId> = t.children(n).collect();
                if let Some(&base) = kids.first() {
                    if self.name_decl(ix, t, base, parent).is_some_and(|d| self.is_real(&d)) {
                        err(n, format!("cannot select bits of real '{}'", t.bare_text(base)));
                    }
                }
                for &i in kids.iter().skip(1) {
                    if self.name_decl(ix, t, i, parent).is_some_and(|d| self.is_real(&d)) {
                        err(i, format!("index '{}' is real", t.bare_text(i)));
                    }
                }
            }
            EventExpr => {
                let tk = toks(t, n);
                if tk.iter().any(|x| matches!(*x, "posedge" | "negedge" | "edge")) {
                    for e in t.children(n) {
                        if self.name_decl(ix, t, e, parent).is_some_and(|d| self.is_real(&d)) {
                            err(e, format!("edge of real '{}'", t.bare_text(e)));
                        }
                    }
                }
            }
            DataType if toks(t, n).first() == Some(&"struct") && toks(t, n).contains(&"packed") => {
                for m in t.children(n).filter(|&c| t.kind(c) == DataDecl) {
                    for d in t.children(m).filter(|&c| t.kind(c) == Declarator) {
                        if toks(t, d).contains(&"=") {
                            err(d, format!("packed struct member '{}' cannot have a default", name_of(t, d)));
                        }
                    }
                }
            }
            DataType if first_tok(t, n) == Some("enum") => self.enum_decl(t, n, &mut err),
            ExprStmt => self.assignment(ix, t, n, parent, &mut err),
            ConcatExpr if matches!(toks(t, n).get(1).copied(), Some("<<" | ">>")) => {
                self.stream_into(ix, t, n, parent, &mut err)
            }
            AssignPattern => array_pattern(t, n, parent, &mut err),
            Declarator => {
                // `e v = 1;`
                let Some(p) = up(parent, n) else { return };
                if t.kind(p) != DataDecl {
                    return;
                }
                let d = Decl { kind: SymKind::Variable, tree: ix, dtype: t.child(p, DataType), array: false };
                if self.is_enum(&d) {
                    if let Some(v) = t.children(n).find(|&c| t.kind(c) == Literal) {
                        err(v, format!("cannot assign an integer to enum '{}' without a cast", name_of(t, n)));
                    }
                }
            }
            ReturnStmt => {
                // In a fork, or with a value in a void function.
                let mut cur = up(parent, n);
                while let Some(a) = cur {
                    match t.kind(a) {
                        ParBlock => {
                            err(n, "return inside a fork".into());
                            return;
                        }
                        FunctionDecl => {
                            let void = t.child(a, ReturnType).and_then(|r| first_tok(t, r)) == Some("void");
                            if void && t.children(n).next().is_some() {
                                err(n, "a void function cannot return a value".into());
                            }
                            return;
                        }
                        TaskDecl | ProceduralBlock | ClassDecl | ModuleDecl => return,
                        _ => {}
                    }
                    cur = up(parent, a);
                }
            }
            ParBlock => {
                let last = t.token_bounds(n).map(|(_, l)| &*t.tokens[l as usize].text);
                if last == Some("join_none") {
                    return;
                }
                let mut cur = up(parent, n);
                while let Some(a) = cur {
                    match t.kind(a) {
                        FunctionDecl => {
                            err(n, "only fork ... join_none is allowed in a function".into());
                            return;
                        }
                        TaskDecl | ProceduralBlock | ClassDecl | ModuleDecl => return,
                        _ => {}
                    }
                    cur = up(parent, a);
                }
            }
            ParamDecl => {
                for d in t.children(n).filter(|&c| t.kind(c) == Declarator) {
                    let mut stack: Vec<NodeId> = t.children(d).filter(|&c| t.kind(c) != Name).collect();
                    while let Some(e) = stack.pop() {
                        if let Some(dc) = self.name_decl(ix, t, e, parent) {
                            if dc.kind == SymKind::Specparam {
                                err(
                                    e,
                                    format!("parameter '{}' cannot use specparam '{}'", name_of(t, d), t.bare_text(e)),
                                );
                            }
                        }
                        stack.extend(t.children(e));
                    }
                }
            }
            ClassDecl => {
                self.randc(t, n, &mut err);
                self.pure_constraints(t, n, &mut err);
            }
            _ => {}
        }
    }

    fn assignment(&self, ix: usize, t: &Tree, stmt: NodeId, parent: &[u32], err: &mut impl FnMut(NodeId, String)) {
        let Some(e) = t.children(stmt).next() else { return };
        if t.kind(e) != BinaryExpr {
            return;
        }
        let op = toks(t, e).first().copied().unwrap_or("");
        if !ASSIGN_OPS.contains(&op) {
            return;
        }
        let kids: Vec<NodeId> = t.children(e).collect();
        if kids.len() != 2 {
            return;
        }
        let (lhs, rhs) = (kids[0], kids[1]);
        // `a = b = c`: the inner assignment needs parentheses.
        for side in [lhs, rhs] {
            if t.kind(side) == BinaryExpr && toks(t, side).first().is_some_and(|o| ASSIGN_OPS.contains(o) && *o != "<=")
            {
                err(side, "an assignment inside an expression must be in parentheses".into());
                return;
            }
        }
        // Procedural code only (not `assign`, `force`).
        let mut procedural = false;
        let mut cur = up(parent, stmt);
        while let Some(a) = cur {
            match t.kind(a) {
                ProceduralBlock => {
                    procedural = toks(t, a).first().is_some_and(|k| PROCEDURAL.contains(k));
                    break;
                }
                FunctionDecl | TaskDecl => {
                    procedural = true;
                    break;
                }
                ProcAssign | ContinuousAssign | ClassDecl | ModuleDecl | InterfaceDecl | ProgramDecl => break,
                _ => {}
            }
            cur = up(parent, a);
        }
        let mut target = lhs;
        while t.kind(target) == SelectExpr {
            match t.children(target).next() {
                Some(b) => target = b,
                None => return,
            }
        }
        let Some(d) = self.name_decl(ix, t, target, parent) else { return };
        if procedural && d.kind == SymKind::Net {
            err(lhs, format!("cannot assign net '{}' in procedural code", t.bare_text(target)));
            return;
        }
        if target == lhs && d.kind == SymKind::Variable && !d.array && self.is_enum(&d) {
            if op != "=" && op != "<=" {
                err(e, format!("enum '{}' cannot be used in a compound assignment", t.bare_text(lhs)));
            } else if t.kind(rhs) == Literal {
                err(rhs, format!("cannot assign an integer to enum '{}' without a cast", t.bare_text(lhs)));
            }
        }
    }

    fn enum_decl(&self, t: &Tree, n: NodeId, err: &mut impl FnMut(NodeId, String)) {
        let base = t.child(n, DataType);
        let base_kw = base.and_then(|b| first_tok(t, b)).unwrap_or("int");
        let two_state = matches!(base_kw, "int" | "bit" | "byte" | "shortint" | "longint");
        // The base width when it is plain: keyword times literal ranges.
        let mut width: Option<u64> = match base_kw {
            "logic" | "bit" | "reg" => Some(1),
            "byte" => Some(8),
            "shortint" => Some(16),
            "int" | "integer" => Some(32),
            "longint" => Some(64),
            _ => None,
        };
        if let Some(b) = base {
            for d in t.children(b).filter(|&c| t.kind(c) == Dim) {
                let nums: Vec<i64> =
                    t.token_range(d).iter().filter_map(|&i| t.tokens[i as usize].text.parse().ok()).collect();
                let range_toks = t.token_range(d).len();
                width = match (width, nums.as_slice()) {
                    (Some(w), [a, b]) if range_toks == 5 => Some(w * ((a - b).unsigned_abs() + 1)),
                    _ => None,
                };
            }
        }
        let mut prev_xz = false;
        for d in t.children(n).filter(|&c| t.kind(c) == Declarator) {
            let Some(value) = t.children(d).find(|&c| !matches!(t.kind(c), Name | Dim)) else {
                if prev_xz {
                    err(d, format!("enum name '{}' after an x/z value needs a value", name_of(t, d)));
                }
                continue;
            };
            // x/z anywhere in the value (`{32{1'bx}}`).
            let any_xz = t.preorder_of(value).into_iter().filter(|&l| t.kind(l) == Literal).any(|l| {
                let text = t.bare_text(l);
                text.split_once('\'').is_some_and(|(_, d)| {
                    d.trim_start_matches(['s', 'S'])
                        .get(1..)
                        .unwrap_or("")
                        .chars()
                        .any(|c| matches!(c, 'x' | 'X' | 'z' | 'Z' | '?'))
                })
            });
            let lit = (t.kind(value) == Literal).then(|| t.bare_text(value));
            let Some(lit) = lit else {
                if any_xz && two_state {
                    err(d, format!("enum '{}' has an x/z value but a two-state base type", name_of(t, d)));
                }
                prev_xz = any_xz;
                continue;
            };
            let Some((size, digits)) = lit.split_once('\'') else {
                prev_xz = false;
                continue;
            };
            let digits = digits.trim_start_matches(['s', 'S']);
            let body = digits.get(1..).unwrap_or("");
            let xz = body.chars().any(|c| matches!(c, 'x' | 'X' | 'z' | 'Z' | '?'));
            if xz && two_state {
                err(d, format!("enum '{}' has an x/z value but a two-state base type", name_of(t, d)));
            }
            prev_xz = xz;
            if let (Ok(size), Some(w)) = (size.trim().parse::<u64>(), width) {
                if size != w {
                    err(d, format!("enum value '{}' is {size} bits wide, the base type {w}", name_of(t, d)));
                }
            }
        }
    }

    /// A class that is not virtual must implement every pure constraint
    /// it inherits (18.5.2). Only for a chain of plain class names.
    fn pure_constraints(&self, t: &Tree, class: NodeId, err: &mut impl FnMut(NodeId, String)) {
        let tk = toks(t, class);
        if tk.contains(&"virtual") || tk.contains(&"interface") {
            return;
        }
        let constraints = |t: &Tree, c: NodeId| -> Vec<(String, bool)> {
            t.children(c)
                .filter(|&x| matches!(t.kind(x), ConstraintDecl | ConstraintPrototype))
                .map(|x| (name_of(t, x), toks(t, x).contains(&"pure")))
                .collect()
        };
        let mut defined: Vec<String> = constraints(t, class).into_iter().map(|c| c.0).collect();
        let mut pure: Vec<String> = Vec::new();
        let (mut ct, mut cn) = (t, class);
        for _ in 0..32 {
            let Some(base) = self.base_class(ct, cn) else { break };
            let (bt, bn) = base;
            for (name, is_pure) in constraints(bt, bn) {
                if is_pure {
                    pure.push(name);
                } else {
                    defined.push(name);
                }
            }
            ct = bt;
            cn = bn;
        }
        for p in pure {
            if !defined.contains(&p) {
                err(class, format!("class '{}' does not implement pure constraint '{p}'", name_of(t, class)));
            }
        }
    }

    /// The single class a class extends, when named plainly.
    fn base_class(&self, t: &Tree, class: NodeId) -> Option<(&Tree, NodeId)> {
        let ext = crate::names::extends(t, class)?;
        let tk: Vec<&str> = t.token_range(ext).iter().map(|&i| &*t.tokens[i as usize].text).collect();
        let [_, name] = tk.as_slice() else { return None };
        match self.classes.get(*name).map(|v| v.as_slice()) {
            Some([(i, n)]) => Some((&self.trees[*i], *n)),
            _ => None,
        }
    }

    fn randc(&self, t: &Tree, class: NodeId, err: &mut impl FnMut(NodeId, String)) {
        let mut randc = Vec::new();
        for d in t.children(class).filter(|&c| t.kind(c) == DataDecl) {
            if toks(t, d).contains(&"randc") {
                randc.extend(t.children(d).filter(|&x| t.kind(x) == Declarator).map(|x| name_of(t, x)));
            }
        }
        if randc.is_empty() {
            return;
        }
        let is_randc = |s: &str| randc.iter().any(|r| r == s);
        for c in t.children(class).filter(|&c| t.kind(c) == ConstraintDecl) {
            for item in t.preorder_of(c) {
                match t.kind(item) {
                    ConstraintItem => {
                        let tk = toks(t, item);
                        if tk.first() == Some(&"solve") {
                            for x in tk.iter().filter(|x| is_randc(x)) {
                                err(item, format!("randc variable '{x}' cannot be in solve ... before"));
                            }
                        }
                        if tk.first() == Some(&"soft") {
                            for e in t.preorder_of(item) {
                                if t.kind(e) == NameExpr && is_randc(&t.bare_text(e)) {
                                    err(
                                        e,
                                        format!("randc variable '{}' cannot be in a soft constraint", t.bare_text(e)),
                                    );
                                }
                            }
                        }
                    }
                    ExprOrDist if toks(t, item).contains(&"dist") => {
                        if let Some(e) = t.children(item).next() {
                            if t.kind(e) == NameExpr && is_randc(&t.bare_text(e)) {
                                err(e, format!("randc variable '{}' cannot have a distribution", t.bare_text(e)));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

trait Single {
    fn is_none_or_multi(&self) -> bool;
}

impl Single for Option<(u32, u32)> {
    /// No tokens, or more than one (a dotted or scoped name).
    fn is_none_or_multi(&self) -> bool {
        !self.is_some_and(|(a, b)| a == b)
    }
}

trait PreorderOf {
    fn preorder_of(&self, n: NodeId) -> Vec<NodeId>;
}

impl PreorderOf for Tree {
    fn preorder_of(&self, n: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![n];
        while let Some(x) = stack.pop() {
            out.push(x);
            let kids: Vec<NodeId> = self.children(x).collect();
            stack.extend(kids.into_iter().rev());
        }
        out
    }
}
