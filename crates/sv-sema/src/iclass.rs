//! Class rules of builder phase N5 that need the classes' relations:
//!
//! - `C::name` of a parameterized class `C` without `#(...)`, outside `C`
//!   and its out-of-block methods (8.25.1);
//! - `implements T` of a type parameter (8.26.4);
//! - `implements I` of an interface class only forward-declared there,
//!   defined later (8.26.4);
//! - two methods of the same name with different prototypes inherited from
//!   the interface classes a class implements or an interface class
//!   extends, also two specializations of one interface class (8.26.6.1,
//!   8.26.6.3);
//! - a type name (typedef or type parameter) inherited from two interface
//!   classes and not declared again by the one extending them (8.26.6.2).
//!
//! A class or base the checks cannot find (not in the design, or declared
//! twice) is never an error.

use crate::fx::{FxMap as HashMap, FxSet as HashSet};

use sv_syntax::{Child, NodeId, SyntaxKind, Tree};
use SyntaxKind::*;

use crate::names::Diag;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct At {
    tree: usize,
    node: NodeId,
}

fn text(t: &Tree, n: NodeId) -> String {
    let mut s = String::new();
    for i in t.token_range(n) {
        if !s.is_empty() {
            s.push(' ');
        }
        s.push_str(&t.tokens[i as usize].text);
    }
    s
}

fn name_of(t: &Tree, n: NodeId) -> String {
    t.child(n, Name).map(|c| t.bare_text(c)).unwrap_or_default()
}

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

/// A class's header lists: `extends ...` and `implements ...` items.
fn header_lists(t: &Tree, class: NodeId, kw: &str) -> Vec<NodeId> {
    let header = t.child(class, Header);
    t.children(class)
        .chain(header.into_iter().flat_map(|h| t.children(h)))
        .filter(|&c| t.kind(c) == Extends && own_toks(t, c).first() == Some(&kw))
        .flat_map(|e| t.children(e).filter(|&d| t.kind(d) == DataType).collect::<Vec<_>>())
        .collect()
}

/// The parameters of a class header: (name, is type, default text).
fn params(t: &Tree, class: NodeId) -> Vec<(String, bool, Option<String>)> {
    let mut out = Vec::new();
    let Some(pl) = t.child(class, Header).and_then(|h| t.child(h, ParamList)) else { return out };
    let mut ty = false;
    for p in t.children(pl).filter(|&p| t.kind(p) == ParamDecl) {
        let own = own_toks(t, p);
        if own.contains(&"type") {
            ty = true;
        } else if own.iter().any(|x| matches!(*x, "parameter" | "localparam")) || t.child(p, DataType).is_some() {
            ty = false;
        }
        if let Some(n) = t.child(p, Name) {
            let d = t.child(p, DataType).map(|d| text(t, d));
            out.push((t.bare_text(n), ty, d));
        }
        for d in t.children(p).filter(|&d| t.kind(d) == Declarator) {
            let v = t.children(d).find(|&c| !matches!(t.kind(c), Name | Dim)).map(|v| text(t, v));
            out.push((name_of(t, d), ty, v));
        }
    }
    out
}

/// The class a header item names, and its argument texts by position or name.
fn item(t: &Tree, d: NodeId) -> (String, Vec<(Option<String>, String)>) {
    let toks: Vec<&str> = own_toks(t, d);
    let name = toks.iter().rev().find(|x| **x != "::").map(|x| x.to_string()).unwrap_or_default();
    let mut args = Vec::new();
    if let Some(al) = t.child(d, ParamList).and_then(|p| t.child(p, ArgList)) {
        for a in t.children(al).filter(|&a| t.kind(a) == Arg) {
            let own = own_toks(t, a);
            let v = t.children(a).next().map(|v| text(t, v)).unwrap_or_default();
            let key = (own.first() == Some(&".")).then(|| own.get(1).map(|s| s.to_string())).flatten();
            args.push((key, v));
        }
    }
    (name, args)
}

/// Replaces whole identifiers of `s` by their bindings.
fn subst(s: &str, map: &HashMap<String, String>) -> String {
    s.split(' ').map(|w| map.get(w).cloned().unwrap_or_else(|| w.to_string())).collect::<Vec<_>>().join(" ")
}

struct Classes<'a> {
    trees: &'a [Tree],
    by_name: HashMap<String, Vec<At>>,
}

/// What an interface class brings: methods by name (prototype text) and
/// type names (where declared, with the specialization).
#[derive(Default)]
struct Inherited {
    methods: HashMap<String, HashSet<String>>,
    types: HashMap<String, HashSet<String>>,
}

impl Classes<'_> {
    fn unique(&self, name: &str) -> Option<At> {
        match self.by_name.get(name)?.as_slice() {
            [a] => Some(*a),
            _ => None,
        }
    }

    /// The bindings of a class's parameters for a header item's arguments.
    fn bind(
        &self,
        c: At,
        args: &[(Option<String>, String)],
        outer: &HashMap<String, String>,
    ) -> HashMap<String, String> {
        let t = &self.trees[c.tree];
        let mut map = HashMap::default();
        for (k, (name, _, default)) in params(t, c.node).into_iter().enumerate() {
            let v = args
                .iter()
                .find(|(key, _)| key.as_deref() == Some(name.as_str()))
                .or_else(|| args.get(k).filter(|(key, _)| key.is_none()))
                .map(|(_, v)| subst(v, outer));
            let v = v.or_else(|| default.map(|d| subst(&d, &map)));
            if let Some(v) = v {
                map.insert(name, v);
            }
        }
        map
    }

    /// Everything interface class `c` (specialized by `map`) declares and
    /// inherits; `None` if a base cannot be found.
    fn inherited(&self, c: At, map: &HashMap<String, String>, depth: usize, into: &mut Inherited) -> Option<()> {
        if depth > 16 {
            return None;
        }
        let t = &self.trees[c.tree];
        let cname = name_of(t, c.node);
        let spec: String = {
            let mut v: Vec<_> = map.iter().map(|(k, v)| format!("{k}={v}")).collect();
            v.sort();
            v.join(",")
        };
        for m in t.children(c.node) {
            match t.kind(m) {
                MethodPrototype => {
                    let name = name_of(t, m);
                    let ret = t.child(m, ReturnType).map(|r| text(t, r)).unwrap_or_default();
                    let mut sig = vec![subst(&ret, map)];
                    if let Some(pl) = t.child(m, PortList) {
                        let mut dt = String::new();
                        for p in t.children(pl).filter(|&p| t.kind(p) == Port) {
                            if let Some(d) = t.child(p, DataType) {
                                dt = subst(&text(t, d), map);
                            }
                            sig.push(dt.clone());
                        }
                    }
                    into.methods.entry(name).or_default().insert(sig.join("|"));
                }
                TypedefDecl => {
                    into.types.entry(name_of(t, m)).or_default().insert(format!("{cname}#({spec})"));
                }
                _ => {}
            }
        }
        for (name, ty, _) in params(t, c.node) {
            if ty {
                into.types.entry(name).or_default().insert(format!("{cname}#({spec})"));
            }
        }
        for d in header_lists(t, c.node, "extends") {
            let (b, args) = item(t, d);
            let bc = self.unique(&b)?;
            let bmap = self.bind(bc, &args, map);
            self.inherited(bc, &bmap, depth + 1, into)?;
        }
        Some(())
    }
}

/// Runs this module's checks over every tree of a compile and returns the
/// errors found (interface class and class relation errors), in tree and source order.
pub fn check(trees: &[Tree]) -> Vec<Diag> {
    let mut by_name: HashMap<String, Vec<At>> = HashMap::default();
    for (ix, t) in trees.iter().enumerate() {
        for n in t.preorder() {
            if t.kind(n) == ClassDecl {
                by_name.entry(name_of(t, n)).or_default().push(At { tree: ix, node: n });
            }
        }
    }
    let cx = Classes { trees, by_name };
    let mut out = Vec::new();
    for (ix, t) in trees.iter().enumerate() {
        let parent = parents(t);
        for n in t.preorder() {
            match t.kind(n) {
                ClassDecl => class_rules(&cx, ix, t, n, &mut out),
                ScopedExpr => unadorned(&cx, ix, t, n, &parent, &mut out),
                _ => {}
            }
        }
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

fn class_rules(cx: &Classes, ix: usize, t: &Tree, n: NodeId, out: &mut Vec<Diag>) {
    let own = own_toks(t, n);
    let interface = own.contains(&"interface");
    let cname = name_of(t, n);
    let my_params = params(t, n);
    let implements = header_lists(t, n, "implements");
    for &d in &implements {
        let (name, _) = item(t, d);
        if my_params.iter().any(|(p, ty, _)| *ty && *p == name) {
            out.push(Diag {
                tree: ix,
                node: d,
                message: format!("'{name}' is a type parameter; a class can only implement an interface class"),
            });
            continue;
        }
        // Defined after this class, with only a forward typedef before it.
        if let Some(c) = cx.unique(&name) {
            if c.tree == ix && t.token_bounds(c.node).map(|b| b.0) > t.token_bounds(n).map(|b| b.0) {
                out.push(Diag {
                    tree: ix,
                    node: d,
                    message: format!("interface class '{name}' is implemented before it is defined"),
                });
            }
        }
    }
    // What the implemented (or, for an interface class, extended) classes bring.
    let bases = if interface { header_lists(t, n, "extends") } else { implements };
    if bases.len() < 2 {
        return;
    }
    let mut all = Inherited::default();
    for &d in &bases {
        let (b, args) = item(t, d);
        let Some(bc) = cx.unique(&b) else { return };
        let map = cx.bind(bc, &args, &HashMap::default());
        if cx.inherited(bc, &map, 0, &mut all).is_none() {
            return;
        }
    }
    let mut mine: HashSet<String> = my_params.iter().map(|p| p.0.clone()).collect();
    for m in t.children(n) {
        if matches!(t.kind(m), TypedefDecl | FunctionDecl | TaskDecl | MethodPrototype) {
            mine.insert(name_of(t, m));
        }
    }
    let mut names: Vec<&String> = all.methods.keys().collect();
    names.sort();
    for m in names {
        if all.methods[m].len() > 1 {
            out.push(Diag {
                tree: ix,
                node: n,
                message: format!("'{cname}' inherits method '{m}' with different prototypes"),
            });
        }
    }
    let mut tnames: Vec<&String> = all.types.keys().collect();
    tnames.sort();
    for ty in tnames {
        if all.types[ty].len() > 1 && !mine.contains(ty) {
            out.push(Diag {
                tree: ix,
                node: n,
                message: format!("'{cname}' inherits type '{ty}' from more than one interface class"),
            });
        }
    }
}

/// `C::x` of a parameterized class without `#(...)`.
fn unadorned(cx: &Classes, ix: usize, t: &Tree, n: NodeId, parent: &[u32], out: &mut Vec<Diag>) {
    let Some(first) = t.children(n).next() else { return };
    if t.kind(first) != NameExpr {
        return;
    }
    let name = t.bare_text(first);
    let Some(c) = cx.unique(&name) else { return };
    let ct = &cx.trees[c.tree];
    if params(ct, c.node).is_empty() {
        return;
    }
    // Inside the class, its out-of-block methods, or a class naming it.
    let mut cur = parent[n.0 as usize];
    while cur != u32::MAX {
        let a = NodeId(cur);
        if c.tree == ix && a == c.node {
            return;
        }
        match t.kind(a) {
            ClassDecl => {
                let h = t.child(a, Header).map(|h| text(t, h)).unwrap_or_default();
                if h.split(' ').any(|w| w == name) {
                    return;
                }
            }
            FunctionDecl | TaskDecl | ConstraintDecl => {
                if t.child(a, Name).is_some_and(|nm| text(t, nm).split(' ').any(|w| w == name)) {
                    return;
                }
            }
            _ => {}
        }
        cur = parent[cur as usize];
    }
    out.push(Diag {
        tree: ix,
        node: first,
        message: format!("parameterized class '{name}' needs a specialization ('{name}#()') here"),
    });
}
