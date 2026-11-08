//! Checks that need the declared type of a name (IEEE 1800 chapters 6.19,
//! 7, 8 and 26): the member a `.` or `::` names exists, and built-in
//! methods and declared subroutines get as many arguments as they take.
//!
//! - `p::x`: `x` is declared in package `p` (26.3);
//! - `s.m` on a struct or union variable: `m` is one of its members (7.2);
//! - `c.m` on a class handle: `m` is a member of the class or of a class
//!   it extends, or a built-in class method (8);
//! - enum, string, queue, associative and dynamic array methods: their
//!   argument counts (6.19.5, 6.16, 7.5 - 7.10);
//! - a call of a function or task by its simple name: as many arguments
//!   as it has formals, fewer only where formals have defaults (13.5).
//!
//! A name is typed only when it resolves to a declaration in a design
//! unit, package, class or subroutine scope with nothing nearer hiding it,
//! and a class only when every class it extends is in the design. A
//! package that exports anything, a parameterized class and named or
//! defaulted arguments are not checked.

use crate::fx::{FxMap as HashMap, FxSet as HashSet};

use sv_syntax::{Child, NodeId, SyntaxKind, Tree};
use SyntaxKind::*;

use crate::names::Diag;
use crate::scopes::{Scopes, SymKind, ROOT, UNIT};

const CLASS_BUILTINS: &[&str] =
    &["randomize", "srandom", "get_randstate", "set_randstate", "rand_mode", "constraint_mode", "new"];

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

/// Arguments between the parentheses of an argument list.
fn count(t: &Tree, n: NodeId) -> u32 {
    let mut commas = 0;
    let mut any = false;
    for c in &t.node(n).children {
        match c {
            Child::Token(i) if &*t.tokens[*i as usize].text == "," => commas += 1,
            Child::Node(_) => any = true,
            _ => {}
        }
    }
    if commas == 0 && !any {
        0
    } else {
        commas + 1
    }
}

fn arg_error(name: &str, (lo, hi): (u32, u32), n: u32) -> Option<String> {
    if n < lo {
        Some(format!("too few arguments for '{name}'; expected {lo} but {n} were provided"))
    } else if n > hi {
        Some(format!("too many arguments for '{name}'; expected {hi} but {n} were provided"))
    } else {
        None
    }
}

/// What a type string is, for its built-in methods.
#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Enum,
    Str,
    Queue,
    Assoc,
    Dynamic,
}

fn shape(ty: &str) -> Option<Shape> {
    if ty.starts_with("enum{") {
        return Some(Shape::Enum);
    }
    if ty == "string" {
        return Some(Shape::Str);
    }
    // The outermost unpacked dimension is the last `$[...]`.
    let at = ty.rfind("$[")?;
    let dim = ty[at + 2..].strip_suffix(']')?;
    if dim.contains('[') {
        return None;
    }
    Some(if dim.is_empty() {
        Shape::Dynamic
    } else if dim == "$" || dim.starts_with("$:") {
        Shape::Queue
    } else if dim.contains(':') {
        return None;
    } else {
        Shape::Assoc
    })
}

fn method_arity(s: Shape, m: &str) -> Option<(u32, u32)> {
    use Shape::*;
    Some(match (s, m) {
        (Enum, "first" | "last" | "num" | "name") => (0, 0),
        (Enum, "next" | "prev") => (0, 1),
        (Str, "len" | "toupper" | "tolower" | "atoi" | "atohex" | "atooct" | "atobin" | "atoreal") => (0, 0),
        (Str, "getc" | "compare" | "icompare" | "itoa" | "hextoa" | "octtoa" | "bintoa" | "realtoa") => (1, 1),
        (Str, "putc" | "substr") => (2, 2),
        (Queue, "size" | "pop_front" | "pop_back") => (0, 0),
        (Queue, "push_front" | "push_back") => (1, 1),
        (Queue, "insert") => (2, 2),
        (Queue, "delete") => (0, 1),
        (Assoc, "num" | "size") => (0, 0),
        (Assoc, "delete") => (0, 1),
        (Assoc, "exists" | "first" | "last" | "next" | "prev") => (1, 1),
        (Dynamic, "size" | "delete") => (0, 0),
        (Queue | Assoc | Dynamic, "reverse" | "sort" | "rsort" | "shuffle") if m == "reverse" || m == "shuffle" => {
            (0, 0)
        }
        _ => return None,
    })
}

/// Member names of a struct or union type string
/// (`struct{int a;logic b;}m.s$1`).
fn struct_members(ty: &str) -> Option<Vec<String>> {
    if !(ty.starts_with("struct") || ty.starts_with("union")) {
        return None;
    }
    let open = ty.find('{')?;
    let mut depth = 0;
    let mut close = None;
    for (i, c) in ty.char_indices().skip(open) {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &ty[open + 1..close?];
    let mut out = Vec::new();
    let mut depth = 0;
    let mut cur = String::new();
    for c in body.chars() {
        match c {
            '{' | '(' => depth += 1,
            '}' | ')' => depth -= 1,
            ';' | ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    out.push(cur);
    Some(
        out.into_iter()
            .filter(|m| !m.trim().is_empty())
            .filter_map(|m| {
                let last = m.trim().rsplit(' ').next()?.to_string();
                Some(last.split(['$', '[']).next().unwrap_or("").to_string())
            })
            .collect(),
    )
}

struct Ctx<'a> {
    trees: &'a [Tree],
    scopes: &'a Scopes,
    types: Vec<Option<String>>,
    /// Symbols by (scope, name).
    by_name: HashMap<(usize, &'a str), Vec<usize>>,
    /// Scope opened by each declaration node.
    opener: HashMap<(usize, NodeId), usize>,
    /// Typedefs by path (`m.e_t`).
    typedefs: HashMap<String, usize>,
    /// Class and package scopes by name (when the name is unique).
    classes: HashMap<String, Option<usize>>,
    packages: HashMap<String, Option<usize>>,
    /// Base class names by class scope.
    bases: HashMap<usize, Option<String>>,
    exporting: HashSet<usize>,
    /// Names each scope declares.
    members: HashMap<usize, Vec<&'a str>>,
}

/// Runs this module's checks over every tree of a compile and returns the
/// errors found (members and arity checked against declared types), in tree and source order.
pub fn check(trees: &[Tree], scopes: &Scopes) -> Vec<Diag> {
    let (types, _) = crate::symbol_info(trees, scopes);
    let mut by_name: HashMap<(usize, &str), Vec<usize>> = HashMap::default();
    let mut typedefs = HashMap::default();
    let mut members: HashMap<usize, Vec<&str>> = HashMap::default();
    for (i, s) in scopes.symbols.iter().enumerate() {
        members.entry(s.scope).or_default().push(s.name.as_str());
        by_name.entry((s.scope, s.name.as_str())).or_default().push(i);
        if s.kind == SymKind::Typedef {
            typedefs.insert(format!("{}.{}", scopes.scopes[s.scope].path, s.name), i);
        }
    }
    let mut opener = HashMap::default();
    let mut classes: HashMap<String, Option<usize>> = HashMap::default();
    let mut packages: HashMap<String, Option<usize>> = HashMap::default();
    let mut bases = HashMap::default();
    let mut exporting = HashSet::default();
    for (i, sc) in scopes.scopes.iter().enumerate() {
        let Some(y) = sc.symbol else { continue };
        let y = &scopes.symbols[y];
        opener.insert((y.tree, y.node), i);
        let t = &trees[y.tree];
        match y.kind {
            SymKind::Class => {
                let e = classes.entry(y.name.clone()).or_insert(Some(i));
                if *e != Some(i) {
                    *e = None;
                }
                let generic = t.child(y.node, Header).and_then(|h| t.child(h, ParamList)).is_some();
                let base = t.child(y.node, Header).and_then(|h| t.child(h, Extends)).map(|e| {
                    t.token_range(e)
                        .into_iter()
                        .map(|k| &*t.tokens[k as usize].text)
                        .find(|x| *x != "extends")
                        .unwrap_or("")
                        .to_string()
                });
                // A parameterized class, or one extending a specialization:
                // members depend on parameters; not checked.
                let special = t
                    .child(y.node, Header)
                    .and_then(|h| t.child(h, Extends))
                    .is_some_and(|e| t.token_range(e).into_iter().any(|k| &*t.tokens[k as usize].text == "#"));
                if generic || special {
                    classes.insert(y.name.clone(), None);
                }
                bases.insert(i, base);
            }
            SymKind::Package => {
                let e = packages.entry(y.name.clone()).or_insert(Some(i));
                if *e != Some(i) {
                    *e = None;
                }
                if t.children(y.node).any(|n| t.kind(n) == ExportDecl) {
                    exporting.insert(i);
                }
            }
            _ => {}
        }
    }
    let cx = Ctx { trees, scopes, types, by_name, opener, typedefs, classes, packages, bases, exporting, members };
    let mut out = Vec::new();
    for (ix, t) in trees.iter().enumerate() {
        let mut parent = vec![u32::MAX; t.nodes.len()];
        for n in t.preorder() {
            for c in t.children(n) {
                parent[c.0 as usize] = n.0;
            }
        }
        for n in t.preorder() {
            cx.node(ix, t, n, &parent, &mut out);
        }
    }
    out
}

fn up(parent: &[u32], n: NodeId) -> Option<NodeId> {
    let p = parent[n.0 as usize];
    (p != u32::MAX).then_some(NodeId(p))
}

impl<'a> Ctx<'a> {
    /// The symbol a simple name at `at` resolves to, when nothing nearer
    /// (a block declaration, an unknown base class, an import) may hide it.
    fn resolve(&self, ix: usize, t: &Tree, at: NodeId, name: &str, parent: &[u32]) -> Option<usize> {
        let mut cur = up(parent, at);
        let mut scope = None;
        while let Some(a) = cur {
            if let Some(&s) = self.opener.get(&(ix, a)) {
                scope = Some(s);
                break;
            }
            // Declarations in a block, a loop, a `with` clause.
            match t.kind(a) {
                WithClause | ConstraintBlock | LoopStmt => return None,
                _ => {}
            }
            if t.children(a).any(|d| {
                matches!(t.kind(d), DataDecl | ParamDecl)
                    && t.children(d)
                        .any(|x| t.kind(x) == Declarator && t.child(x, Name).is_some_and(|nm| t.bare_text(nm) == name))
            }) {
                return None;
            }
            cur = up(parent, a);
        }
        let mut s = scope.unwrap_or(UNIT);
        loop {
            if s == ROOT {
                return None;
            }
            if let Some(ids) = self.by_name.get(&(s, name)) {
                let ids: Vec<usize> =
                    ids.iter().copied().filter(|&i| s != UNIT || self.scopes.symbols[i].tree == ix).collect();
                return (ids.len() == 1).then(|| ids[0]);
            }
            // An unknown member of a derived class may be inherited.
            if self.bases.get(&s).is_some_and(|b| b.is_some()) {
                return None;
            }
            // Imports could provide it.
            if let Some(y) = self.scopes.scopes[s].symbol {
                let y = &self.scopes.symbols[y];
                let tt = &self.trees[y.tree];
                if tt.children(y.node).any(|c| tt.kind(c) == ImportDecl) {
                    return None;
                }
            }
            s = self.scopes.scopes[s].parent?;
        }
    }

    /// A type string with typedef names replaced by what they name.
    fn resolved_type(&self, sym: usize) -> Option<String> {
        let mut ty = self.types.get(sym)?.clone()?;
        for _ in 0..8 {
            match self.typedefs.get(&ty) {
                Some(&d) => ty = self.types.get(d)?.clone()?,
                None => break,
            }
        }
        Some(ty)
    }

    fn sym_of_name(&self, ix: usize, t: &Tree, e: NodeId, parent: &[u32]) -> Option<usize> {
        if t.kind(e) != NameExpr {
            return None;
        }
        let (a, b) = t.token_bounds(e)?;
        if a != b {
            return None;
        }
        let name = &*t.tokens[a as usize].text;
        let s = self.resolve(ix, t, e, name, parent)?;
        matches!(
            self.scopes.symbols[s].kind,
            SymKind::Variable | SymKind::Net | SymKind::Argument | SymKind::Port | SymKind::Parameter
        )
        .then_some(s)
    }

    /// Members of a class and the classes it extends; None when one of
    /// them is not in the design.
    fn class_members(&self, name: &str) -> Option<HashSet<&'a str>> {
        let mut out = HashSet::default();
        let mut cur = Some(name.to_string());
        let mut seen = HashSet::default();
        while let Some(c) = cur {
            if !seen.insert(c.clone()) {
                break;
            }
            let s = (*self.classes.get(&c)?)?;
            out.extend(self.members.get(&s).into_iter().flatten().copied());
            cur = self.bases.get(&s)?.clone();
        }
        Some(out)
    }

    fn node(&self, ix: usize, t: &Tree, n: NodeId, parent: &[u32], out: &mut Vec<Diag>) {
        let mut err = |node: NodeId, message: String| out.push(Diag { tree: ix, node, message });
        match t.kind(n) {
            ScopedExpr => {
                let own = own_toks(t, n);
                let Some(base) = t.children(n).next() else { return };
                if own.len() != 2 || own[0] != "::" || t.kind(base) != NameExpr {
                    return;
                }
                if up(parent, n).is_some_and(|p| t.kind(p) == ScopedExpr) {
                    return;
                }
                let pkg = t.bare_text(base);
                let Some(Some(s)) = self.packages.get(&pkg) else { return };
                if self.exporting.contains(s) || self.classes.contains_key(&pkg) {
                    return;
                }
                let member = own[1];
                if !self.by_name.contains_key(&(*s, member)) {
                    err(n, format!("no member named '{member}' in package '{pkg}'"));
                }
            }
            MemberExpr => {
                let own = own_toks(t, n);
                let Some(base) = t.children(n).next() else { return };
                if own.len() != 2 || own[0] != "." {
                    return;
                }
                let member = own[1];
                let Some(sym) = self.sym_of_name(ix, t, base, parent) else { return };
                let Some(ty) = self.resolved_type(sym) else { return };
                if let Some(ms) = struct_members(&ty) {
                    if !ms.iter().any(|m| m == member) {
                        err(n, format!("no member named '{member}' in '{ty}'"));
                    }
                    return;
                }
                // A class handle.
                if ty.contains(['{', '$', '[', '#', ' ']) {
                    return;
                }
                let cname = ty.rsplit(['.', ':']).next().unwrap_or(&ty);
                let Some(ms) = self.class_members(cname) else { return };
                if !ms.contains(member) && !CLASS_BUILTINS.contains(&member) {
                    err(n, format!("no member named '{member}' in class '{cname}'"));
                }
            }
            CallExpr => {
                let kids: Vec<NodeId> = t.children(n).collect();
                let (Some(&callee), Some(list)) = (kids.first(), t.child(n, ArgList)) else { return };
                // `with` clauses and named arguments: not checked.
                if t.children(list).any(|a| own_toks(t, a).contains(&".")) {
                    return;
                }
                if up(parent, n).is_some_and(|p| t.kind(p) == CallExpr) || t.child(n, WithClause).is_some() {
                    return;
                }
                let given = count(t, list);
                match t.kind(callee) {
                    MemberExpr => {
                        let own = own_toks(t, callee);
                        let Some(base) = t.children(callee).next() else { return };
                        if own.len() != 2 {
                            return;
                        }
                        let Some(sym) = self.sym_of_name(ix, t, base, parent) else { return };
                        let Some(ty) = self.resolved_type(sym) else { return };
                        let Some(sh) = shape(&ty) else { return };
                        if let Some(r) = method_arity(sh, own[1]) {
                            if let Some(m) = arg_error(own[1], r, given) {
                                err(n, m);
                            }
                        }
                    }
                    NameExpr => {
                        let name = t.bare_text(callee);
                        if name.starts_with('$') || name.contains(['.', ':']) {
                            return;
                        }
                        let Some(sym) = self.resolve(ix, t, callee, &name, parent) else { return };
                        let y = &self.scopes.symbols[sym];
                        if !matches!(y.kind, SymKind::Function | SymKind::Task) {
                            return;
                        }
                        let Some(r) = self.formals(sym) else { return };
                        if let Some(m) = arg_error(&name, r, given) {
                            err(n, m);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// (required, total) formals of a subroutine declared with an ANSI
    /// port list (or none at all).
    fn formals(&self, sym: usize) -> Option<(u32, u32)> {
        let y = &self.scopes.symbols[sym];
        let t = &self.trees[y.tree];
        let decl = y.node;
        if !matches!(t.kind(decl), FunctionDecl | TaskDecl | MethodPrototype) {
            return None;
        }
        if own_toks(t, decl).iter().any(|x| matches!(*x, "import" | "export")) {
            return None;
        }
        let Some(pl) = t.child(decl, PortList) else {
            // Non-ANSI declarations in the body: not checked.
            let body = t.child(decl, Body)?;
            let non_ansi = t.children(body).any(|c| {
                t.token_bounds(c).is_some_and(|(a, _)| {
                    matches!(&*t.tokens[a as usize].text, "input" | "output" | "inout" | "ref" | "const")
                })
            });
            return (!non_ansi).then_some((0, 0));
        };
        let ports: Vec<NodeId> = t.children(pl).filter(|&c| t.kind(c) == Port).collect();
        let total = ports.len() as u32;
        let mut required = 0;
        for (i, p) in ports.iter().enumerate() {
            let has_default = t.child(*p, Declarator).is_some_and(|d| own_toks(t, d).contains(&"="));
            if !has_default {
                required = i as u32 + 1;
            }
        }
        Some((required, total))
    }
}
