//! Name binding (builder phase N5): identifiers that name nothing.
//!
//! A simple identifier used as a value or a type must be declared in a
//! scope that encloses it: its class (and the classes it extends, not the
//! interface classes it implements: 8.26.3), its design unit or package,
//! a package it imports, or the compilation unit. Design unit, package
//! and class names are visible everywhere.
//!
//! The binding is deliberately lenient where it cannot be exact, so that
//! a legal design never gets an error:
//!
//! - a name declared anywhere in an enclosing design unit or class counts,
//!   whatever block it is in and wherever it is declared;
//! - a class whose base class, or a scope whose imported package, is not
//!   in the design is not checked at all;
//! - names that may be resolved through the hierarchy are not checked:
//!   called subroutines (13.4: upward), the first name of a dotted path
//!   (it only needs to be declared somewhere), bind directives and
//!   defparams;
//! - names a construct declares by itself: implicit nets of port
//!   connections and continuous assignments, `foreach` loop variables,
//!   iterator names in `with` clauses (whose whole call is skipped),
//!   members of a randomized object in `randomize() with`.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use sv_syntax::TokenKind as Kind;
use sv_syntax::{Child, NodeId, SyntaxKind, Tree};
use SyntaxKind::*;

use crate::names::Diag;

/// Names every scope sees without declaring them.
const BUILTINS: &[&str] = &[
    "this",
    "super",
    "null",
    "std",
    "local",
    "process",
    "semaphore",
    "mailbox",
    "weak_reference",
    "option",
    "type_option",
    "item",
    "index",
    "randomize",
    "new",
    // The queue type a cross's functions return (19.6.1.3).
    "CrossQueueType",
];

type Names = Rc<HashSet<String>>;

/// A scope's own names and its imports (package, item or `None` for `*`).
type Own = Rc<(HashSet<String>, Vec<(String, Option<String>)>)>;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct At {
    tree: usize,
    node: NodeId,
}

struct Binder<'a> {
    trees: &'a [Tree],
    parents: Vec<Vec<u32>>,
    /// Classes, packages by name.
    classes: HashMap<String, Vec<At>>,
    packages: HashMap<String, At>,
    /// Every name declared anywhere, for the first name of a dotted path.
    everywhere: HashSet<String>,
    /// Names visible everywhere: design units, packages, classes, and the
    /// compilation units' own declarations.
    global: HashSet<String>,
    /// What a scope (design unit, package, class, a file's root) sees;
    /// `None` when it cannot be known.
    visible: std::cell::RefCell<HashMap<At, Option<Names>>>,
    own: std::cell::RefCell<HashMap<At, Own>>,
    /// How often each identifier occurs in the design.
    uses: HashMap<String, usize>,
}

fn is_scope(k: SyntaxKind) -> bool {
    matches!(k, ModuleDecl | InterfaceDecl | ProgramDecl | PackageDecl | ClassDecl | CheckerDecl)
}

/// An identifier as it names something: `\\cpu3 ` is `cpu3` (5.6.1).
fn norm(text: &str) -> &str {
    text.strip_prefix('\\').map_or(text, |x| x.trim_end())
}

fn ident_tokens(t: &Tree, n: NodeId) -> impl Iterator<Item = &str> {
    t.node(n).children.iter().filter_map(move |c| match c {
        Child::Token(i) => {
            let tok = &t.tokens[*i as usize];
            matches!(tok.kind, Kind::Identifier | Kind::EscapedIdentifier).then_some(norm(&tok.text))
        }
        Child::Node(_) => None,
    })
}

fn all_idents(t: &Tree, n: NodeId) -> Vec<String> {
    let mut out = Vec::new();
    walk(t, n, |i| {
        let tok = &t.tokens[i as usize];
        if matches!(tok.kind, Kind::Identifier | Kind::EscapedIdentifier) {
            out.push(norm(&tok.text).to_string());
        }
    });
    out
}

fn last_ident(t: &Tree, n: NodeId) -> Option<String> {
    all_idents(t, n).pop()
}

pub fn check(trees: &[Tree]) -> Vec<Diag> {
    let mut b = Binder {
        trees,
        parents: trees.iter().map(parents).collect(),
        classes: HashMap::new(),
        packages: HashMap::new(),
        everywhere: BUILTINS.iter().map(|s| s.to_string()).collect(),
        global: BUILTINS.iter().map(|s| s.to_string()).collect(),
        visible: Default::default(),
        own: Default::default(),
        uses: HashMap::new(),
    };
    for t in trees {
        for tok in &t.tokens {
            if matches!(tok.kind, Kind::Identifier | Kind::EscapedIdentifier) {
                *b.uses.entry(norm(&tok.text).to_string()).or_default() += 1;
            }
        }
    }
    for (ix, t) in trees.iter().enumerate() {
        for n in t.preorder() {
            let k = t.kind(n);
            if is_scope(k) || matches!(k, UdpDecl | ConfigDecl) {
                if let Some(name) = t.child(n, Name).and_then(|x| last_ident(t, x)) {
                    b.global.insert(name.clone());
                    match k {
                        ClassDecl => b.classes.entry(name).or_default().push(At { tree: ix, node: n }),
                        PackageDecl => {
                            b.packages.entry(name).or_insert(At { tree: ix, node: n });
                        }
                        _ => {}
                    }
                }
            }
        }
        let root = At { tree: ix, node: t.root };
        let own = b.own_names(root);
        b.global.extend(own.0.iter().cloned());
    }
    for (ix, t) in trees.iter().enumerate() {
        for n in t.preorder() {
            if is_scope(t.kind(n)) {
                let own = b.own_names(At { tree: ix, node: n });
                b.everywhere.extend(own.0.iter().cloned());
            }
        }
    }
    let everywhere = std::mem::take(&mut b.everywhere);
    b.everywhere = everywhere.union(&b.global).cloned().collect();
    let mut out = Vec::new();
    for (ix, t) in trees.iter().enumerate() {
        let root = At { tree: ix, node: t.root };
        b.walk(ix, t.root, root, &mut out);
    }
    out
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

impl Binder<'_> {
    /// The names a scope declares itself (not inside the classes and
    /// design units in it, whose names it does declare), and its imports.
    fn own_names(&self, s: At) -> Own {
        if let Some(o) = self.own.borrow().get(&s) {
            return o.clone();
        }
        let t = &self.trees[s.tree];
        let mut names = HashSet::new();
        let mut imports = Vec::new();
        let mut stack: Vec<NodeId> = t.children(s.node).collect();
        // A design unit's header: ports, parameters, imports.
        while let Some(n) = stack.pop() {
            let k = t.kind(n);
            if is_scope(k) && n != s.node {
                if let Some(name) = t.child(n, Name).and_then(|x| last_ident(t, x)) {
                    names.insert(name);
                }
                // `class C extends B #(enum {X, Y})`: the enum's names are here.
                if k == ClassDecl {
                    if let Some(ext) = crate::names::extends(t, n) {
                        stack.push(ext);
                    }
                }
                continue;
            }
            match k {
                Name => {
                    if let Some(x) = last_ident(t, n) {
                        names.insert(x);
                    }
                }
                // `enum { A[2], B[3:4] }`: A0, A1, B3, B4.
                Declarator if t.child(n, Dim).is_some() && t.child(n, Name).is_some() => {
                    if let (Some(x), Some(d)) = (t.child(n, Name).and_then(|x| last_ident(t, x)), t.child(n, Dim)) {
                        let nums: Vec<i64> = t
                            .token_range(d)
                            .iter()
                            .filter_map(|&i| t.tokens[i as usize].text.replace('_', "").parse().ok())
                            .collect();
                        let range = match nums[..] {
                            [n] => Some((0, n - 1)),
                            [a, b] => Some((a.min(b), a.max(b))),
                            _ => None,
                        };
                        if let Some((lo, hi)) = range.filter(|(lo, hi)| hi - lo < 4096) {
                            names.extend((lo..=hi).map(|i| format!("{x}{i}")));
                        }
                    }
                }
                EndLabel | StmtLabel | GenvarDecl => names.extend(ident_tokens(t, n).map(|x| x.to_string())),
                LoopStmt if ident_tokens(t, n).next().is_some() => {
                    // `foreach (a[i, j])`: the loop variables.
                    names.extend(ident_tokens(t, n).map(|x| x.to_string()));
                }
                // `matches tagged a '{.v, .w}`: pattern variables.
                IfStmt | CaseStmt
                    if toks_have(t, n, "matches")
                        || t.token_range(n).iter().any(|&i| &*t.tokens[i as usize].text == "matches") =>
                {
                    let r = t.token_range(n);
                    for w in r.windows(2) {
                        let (a, b) = (&t.tokens[w[0] as usize], &t.tokens[w[1] as usize]);
                        if &*a.text == "." && matches!(b.kind, Kind::Identifier | Kind::EscapedIdentifier) {
                            names.insert(norm(&b.text).to_string());
                        }
                    }
                }
                // `s matches '{a:.i} ? i : 0`
                _ if toks_have(t, n, "matches") => {
                    let r = t.token_range(n);
                    for w in r.windows(2) {
                        let (a, b) = (&t.tokens[w[0] as usize], &t.tokens[w[1] as usize]);
                        if &*a.text == "." && matches!(b.kind, Kind::Identifier | Kind::EscapedIdentifier) {
                            names.insert(norm(&b.text).to_string());
                        }
                    }
                }
                ConstraintItem if t.token_bounds(n).is_some_and(|(f, _)| &*t.tokens[f as usize].text == "foreach") => {
                    names.extend(all_idents(t, n));
                }
                ImportDecl => {
                    let text: String = {
                        let mut s = String::new();
                        walk(t, n, |i| s.push_str(&t.tokens[i as usize].text));
                        s
                    };
                    // import a::*, b::x;
                    for part in text.trim_start_matches("import").trim_end_matches(';').split(',') {
                        if let Some((p, item)) = part.split_once("::") {
                            let item = (item != "*").then(|| item.to_string());
                            imports.push((p.trim().to_string(), item));
                        }
                    }
                }
                // Implicit nets: port connections and continuous assignments.
                Instance | GateInst => {
                    for a in t.children(n) {
                        if t.kind(a) == ArgList {
                            for arg in t.children(a) {
                                for e in t.children(arg) {
                                    if t.kind(e) == NameExpr {
                                        names.extend(all_idents(t, e));
                                    }
                                }
                                if t.kind(arg) == NameExpr {
                                    names.extend(all_idents(t, arg));
                                }
                            }
                        }
                    }
                }
                ContinuousAssign => {
                    for e in t.children(n) {
                        if t.kind(e) == BinaryExpr {
                            if let Some(l) = t.children(e).next() {
                                if t.kind(l) == NameExpr {
                                    names.extend(all_idents(t, l));
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            stack.extend(t.children(n));
        }
        let o = Rc::new((names, imports));
        self.own.borrow_mut().insert(s, o.clone());
        o
    }

    /// The scope around `n` (a design unit, package or class; else the root).
    fn scope_of(&self, tree: usize, n: NodeId) -> At {
        let t = &self.trees[tree];
        let mut cur = self.parents[tree][n.0 as usize];
        while cur != u32::MAX {
            if is_scope(t.kind(NodeId(cur))) {
                return At { tree, node: NodeId(cur) };
            }
            cur = self.parents[tree][cur as usize];
        }
        At { tree, node: t.root }
    }

    /// Everything a scope sees, or `None` when that cannot be known.
    fn visible(&self, s: At, depth: usize) -> Option<Names> {
        if depth > 32 {
            return None;
        }
        if let Some(v) = self.visible.borrow().get(&s) {
            return v.clone();
        }
        let v = self.compute(s, depth);
        self.visible.borrow_mut().insert(s, v.clone());
        v
    }

    fn compute(&self, s: At, depth: usize) -> Option<Names> {
        let t = &self.trees[s.tree];
        let own = self.own_names(s);
        let mut v: HashSet<String> = own.0.clone();
        for (p, item) in &own.1 {
            let pk = *self.packages.get(p)?;
            let pv = self.visible(pk, depth + 1)?;
            match item {
                None => v.extend(pv.iter().cloned()),
                Some(i) => {
                    v.insert(i.clone());
                }
            }
        }
        if t.kind(s.node) == ClassDecl {
            // Members of the classes it extends (not the ones it implements).
            if let Some(ext) = crate::names::extends(t, s.node) {
                for base in self.base_names(t, ext) {
                    let at = self.classes.get(&base)?;
                    for &c in at {
                        v.extend(self.visible(c, depth + 1)?.iter().cloned());
                    }
                }
            }
        }
        if s.node != t.root {
            let up = self.scope_of(s.tree, s.node);
            v.extend(self.visible(up, depth + 1)?.iter().cloned());
        }
        if t.kind(s.node) == PackageDecl {
            // A package's own names only (not its file's), as an import sees them.
            return Some(Rc::new(v));
        }
        v.extend(self.global.iter().cloned());
        Some(Rc::new(v))
    }

    /// The class names in `extends A#(..), B` (the last name of each).
    fn base_names(&self, t: &Tree, ext: NodeId) -> Vec<String> {
        let mut out = Vec::new();
        let mut depth = 0;
        let mut last: Option<String> = None;
        walk(t, ext, |i| {
            let tok = &t.tokens[i as usize];
            match &*tok.text {
                "(" | "[" => depth += 1,
                ")" | "]" => depth -= 1,
                "," if depth == 0 => {
                    out.extend(last.take());
                }
                _ if depth == 0 && matches!(tok.kind, Kind::Identifier | Kind::EscapedIdentifier) => {
                    last = Some(tok.text.to_string());
                }
                _ => {}
            }
        });
        out.extend(last);
        out
    }

    fn walk(&self, ix: usize, n: NodeId, scope: At, out: &mut Vec<Diag>) {
        let t = &self.trees[ix];
        let k = t.kind(n);
        let scope = if is_scope(k) { At { tree: ix, node: n } } else { scope };
        if k == ClassDecl && self.unused_generic(t, n) {
            // Slang checks a parameterized class only where it is specialized.
            return;
        }
        match k {
            // Resolved elsewhere, or names of something else.
            BindDirective | Attribute | SpecifyBlock | DisableStmt | UdpDecl | ConfigDecl | WithClause
            | Instantiation | ExportDecl | DpiExport | Extends | ImportDecl | ModportDecl | ClockingDecl | NetAlias => {
                return
            }
            ParamDecl if ident_tokens(t, n).next().is_none() && toks_have(t, n, "defparam") => return,
            CallExpr => {
                // A call with `with`: its iterator names and object members.
                if t.children(n).any(|c| t.kind(c) == WithClause) {
                    return;
                }
                let mut kids = t.children(n);
                if let Some(callee) = kids.next() {
                    let text = {
                        let mut s = String::new();
                        walk(t, callee, |i| s.push_str(&t.tokens[i as usize].text));
                        s
                    };
                    if text.ends_with("randomize") {
                        return;
                    }
                    // The callee may be found upward; a dotted one's object is checked.
                    if t.kind(callee) != NameExpr {
                        self.walk(ix, callee, scope, out);
                    }
                }
                for c in kids {
                    self.walk(ix, c, scope, out);
                }
                return;
            }
            // `my_task;`
            ExprStmt if t.children(n).count() == 1 && t.children(n).all(|c| t.kind(c) == NameExpr) => return,
            FunctionDecl | TaskDecl | ConstraintDecl => {
                // `function T C::f()`, `constraint C::c {...}`: they see class C.
                if let Some(nm) = t.child(n, Name) {
                    let toks: Vec<&str> = t.token_range(nm).iter().map(|&i| norm(&t.tokens[i as usize].text)).collect();
                    if let Some(p) = toks.iter().rposition(|x| *x == "::") {
                        let Some(class) = toks[..p].last().map(|c| c.to_string()) else { return };
                        let class = &class;
                        let Some(at) = self.classes.get(class) else { return };
                        if at.len() != 1 {
                            return;
                        }
                        let Some(cv) = self.visible(at[0], 0) else { return };
                        let Some(sv) = self.visible(scope, 0) else { return };
                        let own = self.own_names(At { tree: ix, node: n });
                        let mut v: HashSet<String> = (*cv).clone();
                        v.extend(sv.iter().cloned());
                        v.extend(own.0.iter().cloned());
                        let v = Rc::new(v);
                        for c in t.children(n) {
                            self.walk_with(ix, c, &v, out);
                        }
                        return;
                    }
                }
            }
            MemberExpr => {
                // `a.b.c`: `a` may be anything declared anywhere.
                let mut kids = t.children(n);
                if let Some(base) = kids.next() {
                    if t.kind(base) == NameExpr {
                        if let Some(name) = single_ident(t, base) {
                            if !self.everywhere.contains(name) {
                                if let Some(v) = self.visible(scope, 0) {
                                    if !v.contains(name) {
                                        out.push(undeclared(ix, base, name));
                                    }
                                }
                            }
                        }
                    } else {
                        self.walk(ix, base, scope, out);
                    }
                }
                for c in kids {
                    self.walk(ix, c, scope, out);
                }
                return;
            }
            AssignPattern => {
                // `'{name: v}`: a member or type name before `:`.
                let mut keyed = Vec::new();
                let ch = &t.node(n).children;
                for (i, c) in ch.iter().enumerate() {
                    if let Child::Node(x) = c {
                        let next_colon =
                            matches!(ch.get(i + 1), Some(Child::Token(j)) if &*t.tokens[*j as usize].text == ":");
                        if next_colon {
                            keyed.push(*x);
                        }
                    }
                }
                for c in t.children(n) {
                    if !keyed.contains(&c) {
                        self.walk(ix, c, scope, out);
                    }
                }
                return;
            }
            NameExpr | DataType => {
                if let Some(name) = single_ident(t, n) {
                    if let Some(v) = self.visible(scope, 0) {
                        if !v.contains(name) {
                            out.push(undeclared(ix, n, name));
                        }
                    }
                    return;
                }
            }
            _ => {}
        }
        for c in t.children(n) {
            self.walk(ix, c, scope, out);
        }
    }

    /// A class with parameters whose name occurs only in its declaration.
    fn unused_generic(&self, t: &Tree, n: NodeId) -> bool {
        if t.child(n, Header).and_then(|h| t.child(h, ParamList)).is_none() {
            return false;
        }
        let Some(name) = t.child(n, Name).and_then(|x| last_ident(t, x)) else { return false };
        let own = t.token_range(n).iter().filter(|&&i| norm(&t.tokens[i as usize].text) == name).count();
        self.uses.get(&name).copied().unwrap_or(0) <= own
    }

    /// [`Binder::walk`] with a fixed set of visible names (an out-of-block
    /// method's body).
    fn walk_with(&self, ix: usize, n: NodeId, v: &Names, out: &mut Vec<Diag>) {
        let t = &self.trees[ix];
        if matches!(t.kind(n), NameExpr | DataType) {
            if let Some(name) = single_ident(t, n) {
                if !v.contains(name) && !self.everywhere.contains(name) {
                    out.push(undeclared(ix, n, name));
                }
                return;
            }
        }
        if matches!(t.kind(n), CallExpr | MemberExpr | AssignPattern | WithClause | Attribute | DisableStmt) {
            return;
        }
        for c in t.children(n) {
            self.walk_with(ix, c, v, out);
        }
    }
}

fn walk(t: &Tree, n: NodeId, mut f: impl FnMut(u32)) {
    for i in t.token_range(n) {
        f(i);
    }
}

fn toks_have(t: &Tree, n: NodeId, s: &str) -> bool {
    t.node(n).children.iter().any(|c| matches!(c, Child::Token(i) if &*t.tokens[*i as usize].text == s))
}

/// The identifier a node consists of, when it is exactly one token.
fn single_ident(t: &Tree, n: NodeId) -> Option<&str> {
    let (a, b) = t.token_bounds(n)?;
    if a != b {
        return None;
    }
    let tok = &t.tokens[a as usize];
    matches!(tok.kind, Kind::Identifier | Kind::EscapedIdentifier).then_some(norm(&tok.text))
}

fn undeclared(tree: usize, node: NodeId, name: &str) -> Diag {
    Diag { tree, node, message: format!("use of undeclared identifier '{name}'") }
}
