//! Class member rules and parameter initializers (IEEE 1800 chapters 6.20
//! and 8), found from the syntax of class declarations:
//!
//! - a parameter declared in a body, or a `localparam` in a parameter
//!   port list, has a value (6.20.1, 6.20.4);
//! - `super` only in a class that extends another (8.15);
//! - a static method or a static property's initializer does not use the
//!   class's non-static properties (8.10, 8.9);
//! - a method defined outside its class returns the type its prototype
//!   declares, and is defined once (8.24);
//! - an override of a virtual method has as many arguments as the
//!   method it overrides, with the same directions, and the same return
//!   type (8.20).
//!
//! Types are compared only when both sides are built-in keyword types,
//! and base classes only when their names are unique in the design, so a
//! legal design never gets an error.

use std::collections::{HashMap, HashSet};

use sv_syntax::{Child, NodeId, SyntaxKind, Tree};
use SyntaxKind::*;

use crate::names::Diag;

const BUILTIN: &[&str] = &[
    "void",
    "int",
    "integer",
    "shortint",
    "longint",
    "byte",
    "bit",
    "logic",
    "reg",
    "real",
    "shortreal",
    "realtime",
    "time",
    "string",
    "chandle",
    "event",
];

fn own_toks(t: &Tree, n: NodeId) -> Vec<&str> {
    t.node(n)
        .children
        .iter()
        .filter_map(|c| match c {
            Child::Token(i) => Some(&*t.tokens[*i as usize].text),
            Child::Node(_) => None,
        })
        .collect()
}

fn all_toks(t: &Tree, n: NodeId) -> Vec<&str> {
    t.token_range(n).into_iter().map(|i| &*t.tokens[i as usize].text).collect()
}

fn name_of(t: &Tree, n: NodeId) -> String {
    t.child(n, Name).map(|c| t.bare_text(c)).unwrap_or_default()
}

/// A built-in return type written as one keyword (`int`, `void`), or
/// none.
fn simple_type(t: &Tree, method: NodeId) -> Option<String> {
    let dt = t.child(method, ReturnType).and_then(|r| t.child(r, DataType))?;
    let tk = all_toks(t, dt);
    (tk.len() == 1 && BUILTIN.contains(&tk[0])).then(|| tk[0].to_string())
}

#[derive(Clone)]
struct Arg {
    name: String,
    dir: String,
}

#[derive(Clone)]
struct Method {
    tree: usize,
    node: NodeId,
    virt: bool,
    /// None: arguments declared in the body (not compared).
    args: Option<Vec<Arg>>,
    ret: Option<String>,
}

struct Class {
    base: Option<String>,
    methods: HashMap<String, Method>,
}

fn method_args(t: &Tree, m: NodeId) -> Option<Vec<Arg>> {
    let Some(pl) = t.child(m, PortList) else {
        // `task t; input a; ...`: not compared.
        let body = t.child(m, Body);
        let non_ansi = body.is_some_and(|b| {
            t.children(b).any(|c| {
                t.token_bounds(c).is_some_and(|(a, _)| {
                    matches!(&*t.tokens[a as usize].text, "input" | "output" | "inout" | "ref" | "const")
                })
            })
        });
        return (!non_ansi).then(Vec::new);
    };
    let mut dir = "input".to_string();
    let mut out = Vec::new();
    for p in t.children(pl).filter(|&c| t.kind(c) == Port) {
        let own = own_toks(t, p);
        if let Some(d) = own.iter().find(|x| matches!(**x, "input" | "output" | "inout" | "ref")) {
            dir = if own.contains(&"const") { "const ref".into() } else { d.to_string() };
        }
        let name = t.child(p, Declarator).map(|d| name_of(t, d)).unwrap_or_default();
        out.push(Arg { name, dir: dir.clone() });
    }
    Some(out)
}

fn is_virtual(t: &Tree, m: NodeId) -> bool {
    let own = own_toks(t, m);
    own.contains(&"virtual") || own.contains(&"pure")
}

pub fn check(trees: &[Tree]) -> Vec<Diag> {
    let mut out = Vec::new();
    let mut classes: HashMap<String, Vec<Class>> = HashMap::new();
    for (ix, t) in trees.iter().enumerate() {
        let mut in_header = HashSet::new();
        for n in t.preorder() {
            if t.kind(n) == ParamList {
                in_header.extend(t.children(n));
            }
        }
        for n in t.preorder() {
            match t.kind(n) {
                ParamDecl => params(ix, t, n, in_header.contains(&n), &mut out),
                ClassDecl => {
                    let base = t
                        .child(n, Header)
                        .and_then(|h| t.child(h, Extends))
                        .map(|e| all_toks(t, e).into_iter().find(|x| *x != "extends").unwrap_or("").to_string());
                    let mut methods = HashMap::new();
                    for m in t.children(n).filter(|&c| matches!(t.kind(c), FunctionDecl | TaskDecl | MethodPrototype)) {
                        let name = name_of(t, m);
                        if name.is_empty() || name.contains("::") {
                            continue;
                        }
                        methods.insert(
                            name,
                            Method {
                                tree: ix,
                                node: m,
                                virt: is_virtual(t, m),
                                args: method_args(t, m),
                                ret: simple_type(t, m),
                            },
                        );
                    }
                    classes.entry(name_of(t, n)).or_default().push(Class { base, methods });
                    statics(ix, t, n, &mut out);
                }
                _ => {}
            }
        }
    }
    let unique = |name: &str| classes.get(name).filter(|v| v.len() == 1).map(|v| &v[0]);
    for (ix, t) in trees.iter().enumerate() {
        supers(ix, t, &|name| unique(name).map(|c| c.base.is_some()), &mut out);
    }
    out_of_block(trees, &unique, &mut out);
    // Overrides of virtual methods.
    for list in classes.values() {
        for c in list {
            for (name, m) in &c.methods {
                if name == "new" {
                    continue;
                }
                // The nearest base that declares it, and whether any does
                // virtually.
                let mut seen = HashSet::new();
                let mut base = c.base.clone();
                let mut found: Option<&Method> = None;
                let mut virt = false;
                let mut known = true;
                while let Some(b) = base {
                    if !seen.insert(b.clone()) {
                        break;
                    }
                    let Some(bc) = unique(&b) else {
                        known = false;
                        break;
                    };
                    if let Some(bm) = bc.methods.get(name) {
                        found.get_or_insert(bm);
                        virt |= bm.virt;
                    }
                    base = bc.base.clone();
                }
                let (Some(bm), true, true) = (found, virt, known) else { continue };
                let t = &trees[m.tree];
                let at = t.child(m.node, Name).unwrap_or(m.node);
                if let (Some(a), Some(b)) = (&m.args, &bm.args) {
                    if a.len() != b.len() {
                        out.push(Diag {
                            tree: m.tree,
                            node: at,
                            message: format!(
                                "virtual method '{name}' has different number of arguments from its superclass method"
                            ),
                        });
                        continue;
                    }
                    for (x, y) in a.iter().zip(b) {
                        if x.dir != y.dir {
                            out.push(Diag {
                                tree: m.tree,
                                node: at,
                                message: format!(
                                    "direction of virtual method argument '{}' differs from the direction given in its superclass method",
                                    x.name
                                ),
                            });
                        }
                    }
                }
                if let (Some(r), Some(br)) = (&m.ret, &bm.ret) {
                    if r != br && t.kind(m.node) == trees[bm.tree].kind(bm.node) {
                        out.push(Diag {
                            tree: m.tree,
                            node: at,
                            message: format!(
                                "return type '{r}' of virtual method '{name}' does not match '{br}' declared in its superclass"
                            ),
                        });
                    }
                }
            }
        }
    }
    out
}

/// A body parameter, or a header `localparam`, without a value.
fn params(ix: usize, t: &Tree, n: NodeId, header: bool, out: &mut Vec<Diag>) {
    let own = own_toks(t, n);
    if own.first() == Some(&"defparam") {
        return;
    }
    if header && own.first() != Some(&"localparam") {
        return;
    }
    let decls: Vec<NodeId> = t.children(n).filter(|&c| t.kind(c) == Declarator).collect();
    for d in decls {
        if !own_toks(t, d).contains(&"=") {
            let message = if header {
                "local parameter is missing an initializer"
            } else {
                "parameter declaration is missing an initializer"
            };
            out.push(Diag { tree: ix, node: d, message: message.into() });
        }
    }
    // `localparam type T` in a header: the name is a direct child.
    if header && own.contains(&"type") && !own.contains(&"=") {
        if let Some(nm) = t.child(n, Name) {
            out.push(Diag { tree: ix, node: nm, message: "local parameter is missing an initializer".into() });
        }
    }
}

/// `super` in a class with no base class.
fn supers(ix: usize, t: &Tree, has_base: &dyn Fn(&str) -> Option<bool>, out: &mut Vec<Diag>) {
    let mut parent = vec![u32::MAX; t.nodes.len()];
    for n in t.preorder() {
        for c in t.children(n) {
            parent[c.0 as usize] = n.0;
        }
    }
    let up = |n: NodeId| (parent[n.0 as usize] != u32::MAX).then(|| NodeId(parent[n.0 as usize]));
    for n in t.preorder() {
        if t.kind(n) != NameExpr || t.bare_text(n) != "super" {
            continue;
        }
        let mut cur = up(n);
        let mut class = None;
        while let Some(a) = cur {
            match t.kind(a) {
                // `obj.randomize() with { super.x ... }` binds in obj's class.
                WithClause | ConstraintBlock => break,
                ClassDecl => {
                    let own_base = t.child(a, Header).and_then(|h| t.child(h, Extends)).is_some();
                    class = Some((name_of(t, a), Some(own_base)));
                    break;
                }
                FunctionDecl | TaskDecl => {
                    let nm = name_of(t, a);
                    if let Some((c, _)) = nm.split_once("::") {
                        let c = c.trim().to_string();
                        class = Some((c.clone(), has_base(&c)));
                        break;
                    }
                }
                _ => {}
            }
            cur = up(a);
        }
        if let Some((c, Some(false))) = class {
            out.push(Diag { tree: ix, node: n, message: format!("class '{c}' does not have a super class") });
        }
    }
}

/// Non-static properties used in a static method or a static property's
/// initializer.
fn statics(ix: usize, t: &Tree, class: NodeId, out: &mut Vec<Diag>) {
    let mut props = HashSet::new();
    for d in t.children(class).filter(|&c| t.kind(c) == DataDecl) {
        let own = own_toks(t, d);
        if own.contains(&"static") || own.contains(&"typedef") {
            continue;
        }
        for x in t.children(d).filter(|&c| t.kind(c) == Declarator) {
            props.insert(name_of(t, x));
        }
    }
    props.remove("");
    if props.is_empty() {
        return;
    }
    let mut bodies: Vec<NodeId> = Vec::new();
    for m in t.children(class) {
        let own = own_toks(t, m);
        match t.kind(m) {
            FunctionDecl | TaskDecl => {
                let kw = own.iter().position(|x| matches!(*x, "function" | "task"));
                if own.iter().position(|x| *x == "static").is_some_and(|s| kw.is_some_and(|k| s < k)) {
                    bodies.push(m);
                }
            }
            DataDecl if own.contains(&"static") => {
                for x in t.children(m).filter(|&c| t.kind(c) == Declarator) {
                    bodies.extend(t.children(x).filter(|&c| !matches!(t.kind(c), Name | Dim)));
                }
            }
            _ => {}
        }
    }
    for b in bodies {
        // Names the method declares itself hide the properties.
        let mut local = HashSet::new();
        let mut stack = vec![b];
        let mut uses = Vec::new();
        while let Some(n) = stack.pop() {
            match t.kind(n) {
                Name => {
                    local.insert(t.bare_text(n));
                }
                LoopStmt => {
                    local.extend(own_toks(t, n).into_iter().map(String::from));
                }
                NameExpr => uses.push(n),
                WithClause | ClassDecl => continue,
                // `$bits(a)` and the like do not evaluate their argument.
                CallExpr
                    if t.children(n).next().is_some_and(|c| {
                        matches!(
                            t.bare_text(c).as_str(),
                            "$bits"
                                | "$size"
                                | "$left"
                                | "$right"
                                | "$low"
                                | "$high"
                                | "$increment"
                                | "$dimensions"
                                | "$unpacked_dimensions"
                                | "$typename"
                                | "$isunbounded"
                        )
                    }) =>
                {
                    continue
                }
                _ => {}
            }
            stack.extend(t.children(n));
        }
        for u in uses {
            let name = t.bare_text(u);
            if props.contains(&name) && !local.contains(&name) {
                out.push(Diag {
                    tree: ix,
                    node: u,
                    message: format!("invalid use of non-static class property '{name}'"),
                });
            }
        }
    }
}

/// Out-of-block method definitions: the prototype's return type, defined
/// once.
fn out_of_block<'a>(trees: &[Tree], unique: &dyn Fn(&str) -> Option<&'a Class>, out: &mut Vec<Diag>) {
    let mut seen: HashSet<(usize, u32, String)> = HashSet::new();
    for (ix, t) in trees.iter().enumerate() {
        let mut parent = HashMap::new();
        for n in t.preorder() {
            for c in t.children(n) {
                parent.insert(c, n);
            }
        }
        for n in t.preorder() {
            if !matches!(t.kind(n), FunctionDecl | TaskDecl) {
                continue;
            }
            let full = name_of(t, n);
            let Some((c, m)) = full.split_once("::") else { continue };
            let (c, m) = (c.trim(), m.trim());
            let key = format!("{c}::{m}");
            let at = t.child(n, Name).unwrap_or(n);
            let scope = parent.get(&n).map_or(u32::MAX, |p| p.0);
            if !seen.insert((ix, scope, key.clone())) {
                out.push(Diag { tree: ix, node: at, message: format!("redefinition of '{key}'") });
                continue;
            }
            let Some(class) = unique(c) else { continue };
            let Some(proto) = class.methods.get(m) else { continue };
            let pt = &trees[proto.tree];
            if pt.kind(proto.node) != MethodPrototype {
                continue;
            }
            if let (Some(r), Some(pr)) = (simple_type(t, n), &proto.ret) {
                if &r != pr {
                    out.push(Diag {
                        tree: ix,
                        node: at,
                        message: format!(
                            "return type '{r}' of method '{m}' does not match '{pr}' declared in its prototype"
                        ),
                    });
                }
            }
        }
    }
}
