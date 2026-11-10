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
//!   as it has formals, fewer only where formals have defaults (13.5);
//! - `inside` (11.4.13): a string and an integral value are not
//!   comparable (string literals are integral);
//! - `solve ... before` (18.5.9): only in a constraint's own block, not in
//!   a `foreach` or `if` inside it; each name a `rand` (not `randc`)
//!   variable of the class, a member of a packed struct is not one;
//! - a modport of the interface a virtual interface names is not a value
//!   (25.5): `vif.mp` in an expression.
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
    /// Each tree's parent of each node (`u32::MAX`: none).
    parents: Vec<Vec<u32>>,
    /// Class scopes by name, parameterized ones too (when the name is unique).
    any_class: HashMap<String, Option<usize>>,
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
    let mut any_class: HashMap<String, Option<usize>> = HashMap::default();
    for (i, sc) in scopes.scopes.iter().enumerate() {
        let Some(y) = sc.symbol else { continue };
        let y = &scopes.symbols[y];
        opener.insert((y.tree, y.node), i);
        let t = &trees[y.tree];
        match y.kind {
            SymKind::Class => {
                let e = any_class.entry(y.name.clone()).or_insert(Some(i));
                if *e != Some(i) {
                    *e = None;
                }
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
    let parents = trees
        .iter()
        .map(|t| {
            let mut parent = vec![u32::MAX; t.nodes.len()];
            for n in t.preorder() {
                for c in t.children(n) {
                    parent[c.0 as usize] = n.0;
                }
            }
            parent
        })
        .collect();
    let cx = Ctx {
        trees,
        scopes,
        types,
        by_name,
        opener,
        typedefs,
        classes,
        packages,
        bases,
        exporting,
        members,
        parents,
        any_class,
    };
    let mut out = Vec::new();
    for (ix, t) in trees.iter().enumerate() {
        let parent = &cx.parents[ix];
        for n in t.preorder() {
            cx.node(ix, t, n, parent, &mut out);
        }
    }
    out
}

/// Integral types as type strings print them (no unpacked dimension).
fn integral(ty: &str) -> bool {
    !ty.contains('$')
        && ["byte", "shortint", "int", "longint", "integer", "bit", "logic", "reg", "time"]
            .iter()
            .any(|k| ty.strip_prefix(k).is_some_and(|r| r.is_empty() || r.starts_with(['[', ' ']) || r == " unsigned"))
}

/// A string, or an unpacked array of strings (`string$[$]`).
fn stringish(ty: &str) -> bool {
    ty == "string" || ty.starts_with("string$")
}

/// The parts of a `solve a, b before c;` list, each as its tokens.
fn solve_items<'t>(toks: &[&'t str]) -> Vec<Vec<&'t str>> {
    let mut out = vec![Vec::new()];
    let mut depth = 0;
    for &x in toks {
        match x {
            "solve" | "before" | ";" if depth == 0 => out.push(Vec::new()),
            "," if depth == 0 => out.push(Vec::new()),
            _ => {
                if matches!(x, "[" | "(" | "{") {
                    depth += 1;
                } else if matches!(x, "]" | ")" | "}") {
                    depth -= 1;
                }
                if let Some(l) = out.last_mut() {
                    l.push(x);
                }
            }
        }
    }
    out.retain(|l| !l.is_empty());
    out
}

/// The names a loop's header declares: `foreach (a[i, j])` its loop
/// variables, `for (int i = 0; ...)` the variables it declares.
fn loop_header(t: &Tree, n: NodeId) -> Vec<&str> {
    let mut head = Vec::new();
    let mut depth = 0;
    for k in t.token_range(n) {
        let x = &*t.tokens[k as usize].text;
        head.push(x);
        match x {
            "(" => depth += 1,
            ")" => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
    }
    let ident = |x: &str| x.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_');
    match head.first() {
        Some(&"foreach") => {
            let Some(open) = head.iter().position(|x| *x == "[") else { return Vec::new() };
            head[open + 1..].iter().take_while(|x| **x != "]").copied().filter(|x| ident(x)).collect()
        }
        Some(&"for") => (1..head.len())
            .filter(|&i| head.get(i + 1) == Some(&"=") && !matches!(head[i - 1], "(" | "," | ";"))
            .map(|i| head[i])
            .collect(),
        _ => Vec::new(),
    }
}

fn up(parent: &[u32], n: NodeId) -> Option<NodeId> {
    let p = parent[n.0 as usize];
    (p != u32::MAX).then_some(NodeId(p))
}

impl<'a> Ctx<'a> {
    /// The symbol a simple name at `at` resolves to, when nothing nearer
    /// (a block declaration, an unknown base class, an import) may hide it.
    fn resolve(&self, ix: usize, t: &Tree, at: NodeId, name: &str, parent: &[u32]) -> Option<usize> {
        self.resolve_at(ix, t, at, name, parent, false)
    }

    /// As [`Ctx::resolve`]; with `loops`, also inside a loop whose header
    /// does not name it.
    fn resolve_at(&self, ix: usize, t: &Tree, at: NodeId, name: &str, parent: &[u32], loops: bool) -> Option<usize> {
        let mut cur = up(parent, at);
        let mut scope = None;
        while let Some(a) = cur {
            if let Some(&s) = self.opener.get(&(ix, a)) {
                scope = Some(s);
                break;
            }
            // `function c::f ... endfunction` (out of its class): f's scope in c.
            if loops && matches!(t.kind(a), FunctionDecl | TaskDecl) {
                if let Some(s) = self.extern_scope(t, a) {
                    scope = Some(s);
                    break;
                }
            }
            // Declarations in a block, a loop, a `with` clause.
            match t.kind(a) {
                LoopStmt if loops => {
                    if loop_header(t, a).contains(&name) {
                        return None;
                    }
                }
                WithClause | ConstraintBlock | LoopStmt => return None,
                _ => {}
            }
            // (`loops`: a block's declarations are its subroutine's symbols;
            // a name declared twice there does not resolve.)
            if !loops
                && t.children(a).any(|d| {
                    matches!(t.kind(d), DataDecl | ParamDecl)
                        && t.children(d).any(|x| {
                            t.kind(x) == Declarator && t.child(x, Name).is_some_and(|nm| t.bare_text(nm) == name)
                        })
                })
            {
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
            let found = self
                .typedefs
                .get(&ty)
                .or_else(|| self.typedefs.get(&format!("$unit.{ty}")))
                .or_else(|| self.typedefs.get(&ty.replace("::", ".")));
            match found {
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
                // `vif.mp`: a modport is not a value (as the base of another
                // select it may name a clocking block or signal: not checked).
                let outer = up(parent, n).is_some_and(|p| t.kind(p) == MemberExpr && t.children(p).next() == Some(n));
                if !outer && self.handle_type(ix, t, base, parent).is_some_and(|ty| self.is_modport(&ty, member)) {
                    err(n, format!("'{member}' cannot be used in an expression"));
                    return;
                }
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
            InsideExpr => {
                let (Some(lhs), Some(list)) = (t.children(n).next(), t.child(n, RangeList)) else { return };
                let Some(lt) = self.expr_type(ix, t, lhs, parent) else { return };
                for e in t.children(list) {
                    let Some(et) = self.expr_type(ix, t, e, parent) else { continue };
                    let msg = if integral(&lt) && stringish(&et) {
                        format!("inside expression of type 'string' is not comparable to common type '{lt}'")
                    } else if lt == "string" && integral(&et) {
                        format!("inside expression of type '{et}' is not comparable to common type 'string'")
                    } else {
                        continue;
                    };
                    err(n, msg);
                    return;
                }
            }
            ConstraintItem => {
                let toks: Vec<&str> = t.token_range(n).into_iter().map(|k| &*t.tokens[k as usize].text).collect();
                if toks.first() != Some(&"solve") {
                    return;
                }
                // In a `foreach` or `if` of the constraint: the block's parent is an item.
                if up(parent, n).and_then(|b| up(parent, b)).is_some_and(|g| t.kind(g) == ConstraintItem) {
                    err(n, "constraint ordering directive is not allowed within a nested constraint block".into());
                }
                let Some(cls) = self.class_of(ix, n) else { return };
                for item in solve_items(&toks) {
                    if let Some(m) = self.solve_target(cls, &item) {
                        err(n, m.to_string());
                    }
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

    /// The scope of a method body written out of its class (`c::f`).
    fn extern_scope(&self, t: &Tree, decl: NodeId) -> Option<usize> {
        let name = t.bare_text(t.child(decl, Name)?);
        let (cls, f) = name.split_once("::")?;
        let cs = (*self.any_class.get(cls.trim())?)?;
        let f = f.trim();
        let sub = self
            .scopes
            .scopes
            .iter()
            .position(|sc| sc.parent == Some(cs) && sc.symbol.is_some_and(|y| self.scopes.symbols[y].name == f));
        Some(sub.unwrap_or(cs))
    }

    /// The class scope a node is in (its innermost class declaration).
    fn class_of(&self, ix: usize, n: NodeId) -> Option<usize> {
        let (t, parent) = (&self.trees[ix], &self.parents[ix]);
        let mut cur = up(parent, n);
        while let Some(a) = cur {
            if t.kind(a) == ClassDecl {
                return self.opener.get(&(ix, a)).copied();
            }
            cur = up(parent, a);
        }
        None
    }

    /// The symbol `name` names among the members of class scope `s` and of
    /// the classes it extends; None when one of them is not in the design.
    fn class_member(&self, mut s: usize, name: &str) -> Option<usize> {
        for _ in 0..64 {
            if let Some(ids) = self.by_name.get(&(s, name)) {
                return (ids.len() == 1).then(|| ids[0]);
            }
            let base = self.bases.get(&s)?.as_ref()?;
            s = (*self.classes.get(base.as_str())?)?;
        }
        None
    }

    /// `rand`, `randc` or neither (empty) on a variable's declaration.
    fn rand_kind(&self, sym: usize) -> Option<&'static str> {
        let y = &self.scopes.symbols[sym];
        if y.kind != SymKind::Variable {
            return None;
        }
        let (t, parent) = (&self.trees[y.tree], &self.parents[y.tree]);
        let mut cur = Some(y.node);
        while let Some(a) = cur {
            if t.kind(a) == DataDecl {
                let own = own_toks(t, a);
                return Some(if own.contains(&"randc") {
                    "randc"
                } else if own.contains(&"rand") {
                    "rand"
                } else {
                    ""
                });
            }
            cur = up(parent, a);
        }
        None
    }

    /// What is wrong with one name of a `solve ... before` list in class
    /// scope `cls`, if anything can be told.
    fn solve_target(&self, cls: usize, item: &[&str]) -> Option<&'static str> {
        // A `randc` name is checks.rs's (class `randc` check).
        const NOT_RAND: &str = "target of 'solve before' constraint must be a random variable";
        let item = match item {
            ["this", ".", rest @ ..] => rest,
            _ => item,
        };
        let root = *item.first()?;
        if !root.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
            return None;
        }
        let sym = self.class_member(cls, root)?;
        if self.rand_kind(sym)?.is_empty() {
            return Some(NOT_RAND);
        }
        // `a.m` (after any selects of `a`): one level is looked at.
        let mut depth = 0;
        let mut member = None;
        for (i, x) in item.iter().enumerate().skip(1) {
            match *x {
                "[" => depth += 1,
                "]" => depth -= 1,
                "." if depth == 0 => {
                    member = item.get(i + 1).copied();
                    break;
                }
                _ => {}
            }
        }
        let member = member?;
        let ty = self.resolved_type(sym)?;
        if ty.starts_with("struct packed") || ty.starts_with("union packed") {
            return Some(NOT_RAND);
        }
        let cname = ty.rsplit(['.', ':']).next().unwrap_or(&ty);
        let cs = (*self.classes.get(cname)?)?;
        self.rand_kind(self.class_member(cs, member)?)?.is_empty().then_some(NOT_RAND)
    }

    /// The type of a variable's name, or of a character of a string
    /// (`s[i]`: byte), when it is known.
    fn expr_type(&self, ix: usize, t: &Tree, e: NodeId, parent: &[u32]) -> Option<String> {
        match t.kind(e) {
            NameExpr => {
                let (a, b) = t.token_bounds(e)?;
                if a != b {
                    return None;
                }
                let s = self.resolve_at(ix, t, e, &t.tokens[a as usize].text, parent, true)?;
                matches!(self.scopes.symbols[s].kind, SymKind::Variable | SymKind::Net | SymKind::Argument)
                    .then(|| self.resolved_type(s))?
            }
            SelectExpr => {
                let kids: Vec<NodeId> = t.children(e).collect();
                if kids.len() != 2 || !own_toks(t, e).iter().all(|x| matches!(*x, "[" | "]")) {
                    return None;
                }
                (self.expr_type(ix, t, kids[0], parent)? == "string").then(|| "byte".to_string())
            }
            _ => None,
        }
    }

    /// The type of a handle: a variable's name or a class member of one
    /// (`cntxt.vif`).
    fn handle_type(&self, ix: usize, t: &Tree, e: NodeId, parent: &[u32]) -> Option<String> {
        match t.kind(e) {
            NameExpr => self.expr_type(ix, t, e, parent),
            MemberExpr => {
                let own = own_toks(t, e);
                if own.len() != 2 || own[0] != "." {
                    return None;
                }
                let ty = self.handle_type(ix, t, t.children(e).next()?, parent)?;
                let cname = ty.rsplit(['.', ':']).next().unwrap_or(&ty);
                let cs = (*self.classes.get(cname)?)?;
                let m = self.class_member(cs, own[1])?;
                matches!(self.scopes.symbols[m].kind, SymKind::Variable).then(|| self.resolved_type(m))?
            }
            _ => None,
        }
    }

    /// `ty` is an interface (as a virtual interface's type prints it) that
    /// declares modport `m`.
    fn is_modport(&self, ty: &str, m: &str) -> bool {
        // A parameterized interface prints its values: `ifc#(W=8)`.
        let ty = ty.split('#').next().unwrap_or(ty);
        let Some(ids) = self.by_name.get(&(ROOT, ty)) else { return false };
        ids.iter().any(|&i| {
            let y = &self.scopes.symbols[i];
            y.kind == SymKind::Interface
                && self.opener.get(&(y.tree, y.node)).is_some_and(|&s| {
                    self.by_name
                        .get(&(s, m))
                        .is_some_and(|v| v.iter().any(|&j| self.scopes.symbols[j].kind == SymKind::Modport))
                })
        })
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
