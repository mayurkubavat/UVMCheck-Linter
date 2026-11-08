//! [`Ast`]: declarations, call sites and class lookups over one syntax tree,
//! for the performance, error-handling, dead-code and security rules.
//!
//! The C++ rules that walk slang's syntax are matched node for node; the
//! ones that walk the AST are matched on the syntax that AST comes from
//! (slang binds names, sv-syntax does not, so each says how it resolves
//! one).

use std::collections::{HashMap, HashSet};

use sv_syntax::{NodeId, SyntaxKind};

use crate::syntax::Cst;

use crate::Sink;

use dv_model::Design;

use SyntaxKind::*;

pub(crate) fn kids(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.tree.children(n).collect()
}

/// Every node under `n` (itself included), in source order.
pub(crate) fn subtree(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.tree.subtree(n)
}

pub(crate) fn last_tok(c: &Cst, n: NodeId) -> Option<u32> {
    c.tree.token_bounds(n).map(|(_, b)| b)
}

pub(crate) fn is_ident(c: &Cst, tok: u32) -> bool {
    matches!(c.tree.tokens[tok as usize].kind, sv_lexer::Kind::Identifier | sv_lexer::Kind::EscapedIdentifier)
}

/// Whether slang parses `n` as a name (`a`, `a.b`, `p::c`, `a[i].b`)
/// rather than an expression on a value: a select on a name is part of the
/// name (`IdentifierSelectName`), not an `ElementSelectExpression`.
pub(crate) fn is_name(c: &Cst, n: NodeId) -> bool {
    match c.kind(n) {
        NameExpr => c
            .first_tok(n)
            .is_some_and(|t| is_ident(c, t) || matches!(c.text_of(t), "this" | "super" | "$unit" | "$root" | "local")),
        ScopedExpr => true,
        MemberExpr | SelectExpr => kids(c, n).first().is_some_and(|&b| is_name(c, b)),
        _ => false,
    }
}

/// What the C++ property-usage visitors collect from a class's methods.
#[derive(Default)]
pub(crate) struct Usage {
    pub(crate) written: HashSet<String>,
    pub(crate) read: HashSet<String>,
}

/// slang's `IdentifierNameSyntax` reads, assignments and increments, as
/// `ClassPropertyUsageVisitor` (`unwritten` false) and
/// `ClassPropertyReadWriteVisitor` (`unwritten` true) record them.
pub(crate) fn usage(c: &Cst, n: NodeId, unwritten: bool, u: &mut Usage) {
    let k = kids(c, n);
    match c.kind(n) {
        BinaryExpr => {
            let op = c.toks(n).next().map(|t| c.text_of(t)).unwrap_or("");
            if matches!(op, "=" | "+=" | "-=" | "*=" | "/=") && k.len() == 2 {
                let lhs = c.last_text(k[0]).to_string();
                if op != "=" {
                    u.read.insert(lhs.clone());
                }
                u.written.insert(lhs);
                usage(c, k[1], unwritten, u);
                return;
            }
        }
        PrefixExpr | PostfixExpr => {
            if let Some(&operand) = k.first() {
                let name = c.last_text(operand).to_string();
                u.written.insert(name.clone());
                if !unwritten {
                    return;
                }
                u.read.insert(name);
            }
        }
        NameExpr => {
            if let Some(t) = c.first_tok(n) {
                let base_of_select =
                    c.parent(n).is_some_and(|p| c.kind(p) == SelectExpr && kids(c, p).first() == Some(&n));
                if is_ident(c, t) && !base_of_select {
                    u.read.insert(c.text_of(t).to_string());
                }
            }
        }
        MemberExpr | ScopedExpr => {
            // The name after `.` or `::` is an IdentifierName when the whole
            // is a name and no select or parameter list follows it.
            let base_of_select = c.parent(n).is_some_and(|p| c.kind(p) == SelectExpr && kids(c, p).first() == Some(&n));
            let whole_name = c.kind(n) == ScopedExpr || k.first().is_some_and(|&b| is_name(c, b));
            if whole_name && !base_of_select && c.tree.child(n, ParamList).is_none() {
                if let Some(t) = last_tok(c, n).filter(|&t| is_ident(c, t)) {
                    u.read.insert(c.text_of(t).to_string());
                }
            }
        }
        LoopStmt if c.keyword(n).is_some_and(|t| c.text_of(t) == "foreach") => {
            // `foreach (a.b[i])`: the array is a name, its identifiers are read.
            let toks: Vec<u32> = c.toks(n).collect();
            for &t in toks.iter().skip(2) {
                if c.text_of(t) == "[" {
                    break;
                }
                if is_ident(c, t) {
                    u.read.insert(c.text_of(t).to_string());
                }
            }
        }
        _ => {}
    }
    for x in k {
        usage(c, x, unwritten, u);
    }
}

/// The class's non-rand properties (name, name token, has initializer) and
/// the usage in its methods.
pub(crate) fn class_properties(c: &Cst, cls: NodeId, unwritten: bool) -> (Vec<(String, u32, bool)>, Usage) {
    let mut props: Vec<(String, u32, bool)> = Vec::new();
    let mut u = Usage::default();
    for item in kids(c, cls) {
        match c.kind(item) {
            DataDecl => {
                if c.toks(item).any(|t| matches!(c.text_of(t), "rand" | "randc")) {
                    continue;
                }
                for d in kids(c, item) {
                    if c.kind(d) != Declarator {
                        continue;
                    }
                    let Some(name) = c.tree.child(d, Name) else { continue };
                    let Some(t) = c.first_tok(name) else { continue };
                    let init = c.tok(d, "=").is_some();
                    let n = c.text_of(t).to_string();
                    // One entry per name; a later declarator wins.
                    props.retain(|p| p.0 != n);
                    props.push((n, t, init));
                }
            }
            FunctionDecl | TaskDecl => usage(c, item, unwritten, &mut u),
            _ => {}
        }
    }
    (props, u)
}

/// A call: `f(...)`, `o.m(...)`, `p::f(...)`, or a bare `$name` (slang
/// calls system subroutines written without parentheses).
pub(crate) struct CallSite {
    pub(crate) node: NodeId,
    /// `getSubroutineName()`: the last name of the callee.
    pub(crate) name: String,
    /// Ordered arguments as written (slang's `arguments()` for system calls).
    pub(crate) args: Vec<Option<NodeId>>,
}

pub(crate) fn call_args(c: &Cst, call: NodeId) -> Vec<Option<NodeId>> {
    let Some(list) = c.tree.child(call, ArgList) else { return Vec::new() };
    let mut out = vec![None];
    for k in c.kids(list) {
        match *k {
            sv_syntax::Child::Token(t) if c.text_of(t) == "," => out.push(None),
            sv_syntax::Child::Node(a) => {
                *out.last_mut().unwrap() = if c.kind(a) == Arg { c.tree.children(a).next() } else { Some(a) }
            }
            _ => {}
        }
    }
    if out.len() == 1 && out[0].is_none() {
        out.clear();
    }
    out
}

pub(crate) fn call_site(c: &Cst, n: NodeId) -> Option<CallSite> {
    match c.kind(n) {
        CallExpr => {
            let callee = kids(c, n).first().copied()?;
            if c.kind(callee) == CallExpr {
                return None; // `x.m(..) with ..`: the inner call is the site
            }
            let name = c.last_text(callee).to_string();
            Some(CallSite { node: n, name, args: call_args(c, n) })
        }
        NameExpr => {
            let t = c.first_tok(n)?;
            if c.tree.tokens[t as usize].kind != sv_lexer::Kind::SystemIdentifier {
                return None;
            }
            if matches!(c.text_of(t), "$" | "$root" | "$unit") {
                return None;
            }
            let p = c.parent(n);
            if p.is_some_and(|p| {
                matches!(c.kind(p), CallExpr | MemberExpr | ScopedExpr) && kids(c, p).first() == Some(&n)
            }) {
                return None;
            }
            Some(CallSite { node: n, name: c.text_of(t).to_string(), args: Vec::new() })
        }
        _ => None,
    }
}

/// slang's `toString()`: each token with its leading trivia (one space for
/// a macro-made token that had any).
pub(crate) fn source_text(c: &Cst, n: NodeId) -> String {
    let mut s = String::new();
    for t in c.tree.token_range(n) {
        let tok = &c.tree.tokens[t as usize];
        if !tok.from_macro {
            s.push_str(c.tree.trivia(t));
        } else if tok.space_before {
            s.push(' ');
        }
        s.push_str(&tok.text);
    }
    s
}

/// A string literal's value (`"a\\tb"` -> `a<tab>b`).
pub(crate) fn string_value(c: &Cst, n: NodeId) -> Option<String> {
    if c.kind(n) != Literal {
        return None;
    }
    let (a, b) = c.tree.token_bounds(n)?;
    if a != b {
        return None;
    }
    let raw = c.text_of(a);
    let inner = raw.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::new();
    let mut it = inner.chars();
    while let Some(ch) = it.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('v') => out.push('\x0b'),
            Some('f') => out.push('\x0c'),
            Some('a') => out.push('\x07'),
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    Some(out)
}

/// The node through parentheses (slang binds no parenthesized expression).
pub(crate) fn unparen(c: &Cst, mut n: NodeId) -> NodeId {
    while c.kind(n) == ParenExpr {
        match kids(c, n).first() {
            Some(&x) => n = x,
            None => break,
        }
    }
    n
}

/// An integer literal's value, when it is one (`300`, `'d300`, `12'h12c`).
pub(crate) fn int_value(c: &Cst, n: NodeId) -> Option<u128> {
    if c.kind(n) != Literal {
        return None;
    }
    let toks = c.tree.token_range(n);
    let text: String = toks.iter().map(|&t| c.text_of(t)).collect::<String>().replace('_', "");
    let digits = match text.find('\'') {
        None => return text.parse().ok(),
        Some(i) => &text[i + 1..],
    };
    let digits = digits.trim_start_matches(['s', 'S']);
    let (radix, rest) = match digits.chars().next()? {
        'd' | 'D' => (10, &digits[1..]),
        'h' | 'H' => (16, &digits[1..]),
        'o' | 'O' => (8, &digits[1..]),
        'b' | 'B' => (2, &digits[1..]),
        _ => return None,
    };
    u128::from_str_radix(rest, radix).ok()
}

/// `swe::isLibrarySymbol` for a class: in package uvm_pkg, named uvm_*
/// (or inside such a class), or declared in uvm_pkg.sv or uvm_mock.svh.
pub(crate) fn is_library_class(c: &Cst, cls: NodeId) -> bool {
    let mut cur = Some(cls);
    while let Some(n) = cur {
        let name = c.tree.child(n, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default();
        if (c.kind(n) == PackageDecl && name == "uvm_pkg") || (c.kind(n) == ClassDecl && name.starts_with("uvm_")) {
            return true;
        }
        cur = c.parent(n);
    }
    let Some(t) = c.tree.child(cls, Name).and_then(|x| c.first_tok(x)) else { return false };
    let file = c.tree.tokens[t as usize].origin.file;
    let path = c.tree.files.get(file as usize).map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    path.contains("uvm_mock.svh") || path.contains("uvm_pkg.sv")
}

/// A class declaration, its name and its methods' (declaration, name).
pub(crate) type ClassMethods = (NodeId, String, Vec<(NodeId, String)>);

/// The user classes of the tree with their methods' declarations (in-class
/// bodies and out-of-block `C::m` bodies; no pure or DPI ones).
pub(crate) fn user_class_methods(c: &Cst) -> Vec<ClassMethods> {
    let mut out = Vec::new();
    let classes: Vec<NodeId> = c.nodes(ClassDecl).into_iter().filter(|&k| !is_library_class(c, k)).collect();
    let mut by_name: HashMap<String, usize> = HashMap::new();
    for &k in &classes {
        let name = c.tree.child(k, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default();
        let mut methods = Vec::new();
        for item in kids(c, k) {
            if matches!(c.kind(item), FunctionDecl | TaskDecl) {
                let m = c.tree.child(item, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default();
                methods.push((item, m));
            }
        }
        by_name.entry(name.clone()).or_insert(out.len());
        out.push((k, name, methods));
    }
    for n in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)) {
        let Some(name) = c.tree.child(n, Name) else { continue };
        let toks = c.tree.token_range(name);
        if toks.len() < 3 || c.text_of(toks[toks.len() - 2]) != "::" {
            continue;
        }
        let class = c.text_of(toks[toks.len() - 3]);
        if let Some(&i) = by_name.get(class) {
            out[i].2.push((n, c.text_of(toks[toks.len() - 1]).to_string()));
        }
    }
    out
}

/// The engine walks the elaborated AST, so code in a parameterized class
/// is visited once per specialization. Maps each class declaration of a
/// tree to that count.
pub(crate) struct Ast {
    pub(crate) visits: HashMap<NodeId, u32>,
}

impl Ast {
    pub(crate) fn new(d: &Design, c: &Cst) -> Ast {
        let by_loc: HashMap<(u32, u32, u32), u32> = d
            .classes()
            .iter()
            .filter(|k| k.is_parameterized)
            .map(|k| ((k.loc.file, k.loc.line, k.loc.column), d.specializations(k.id)))
            .collect();
        let mut visits = HashMap::new();
        if !by_loc.is_empty() {
            for n in c.nodes(ClassDecl) {
                let Some(t) = c.tree.child(n, Name).and_then(|x| c.first_tok(x)) else { continue };
                let l = c.loc(t);
                if let Some(&k) = by_loc.get(&(l.file, l.line, l.column)) {
                    visits.insert(n, k);
                }
            }
        }
        Ast { visits }
    }

    /// Visits of node `n`: the product over the classes around it.
    pub(crate) fn times(&self, c: &Cst, n: NodeId) -> u32 {
        let mut k = 1;
        let mut cur = Some(n);
        while let Some(x) = cur {
            if let Some(&v) = self.visits.get(&x) {
                k *= v;
            }
            cur = c.parent(x);
        }
        k
    }

    /// Repeats the finding just added for each further visit of `n`.
    pub(crate) fn again(&self, s: &mut Sink, c: &Cst, n: NodeId) {
        let k = self.times(c, n);
        if k == 0 {
            s.findings.pop();
            return;
        }
        if let Some(last) = s.findings.last().cloned() {
            for _ in 1..k {
                s.findings.push(last.clone());
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum DeclKind {
    /// A method or block local, a loop variable.
    Local,
    Arg,
    /// A class property.
    Prop,
    /// A module, interface, package or `$unit` variable or port.
    Other,
}

/// A declared value: what it is, its declarator (the identity slang's
/// symbol has) and its data type.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Decl {
    pub(crate) kind: DeclKind,
    pub(crate) item: NodeId,
    pub(crate) dtype: Option<NodeId>,
}

pub(crate) fn declarators_named(c: &Cst, decl: NodeId, name: &str) -> Option<NodeId> {
    kids(c, decl)
        .into_iter()
        .find(|&d| c.kind(d) == Declarator && c.tree.child(d, Name).is_some_and(|x| c.tree.bare_text(x) == name))
}

pub(crate) fn find_in_items(c: &Cst, scope: NodeId, name: &str, kind: DeclKind) -> Option<Decl> {
    for item in kids(c, scope) {
        if c.kind(item) != DataDecl {
            continue;
        }
        if let Some(d) = declarators_named(c, item, name) {
            return Some(Decl { kind, item: d, dtype: c.tree.child(item, DataType) });
        }
    }
    None
}

pub(crate) fn class_named(c: &Cst, name: &str) -> Option<NodeId> {
    c.first_named(ClassDecl, name)
}

/// The class `k` extends, by name, within the tree.
pub(crate) fn base_class(c: &Cst, k: NodeId) -> Option<NodeId> {
    let e = c.tree.child(k, Extends).or_else(|| c.tree.child(c.tree.child(k, Header)?, Extends))?;
    let dt = kids(c, e).into_iter().next()?;
    let toks = c.tree.token_range(dt);
    let end = toks.iter().position(|&t| c.text_of(t) == "#").unwrap_or(toks.len());
    let name = c.text_of(*toks[..end].last()?);
    class_named(c, name).filter(|&b| b != k)
}

/// A property of class `k` or a base of it.
pub(crate) fn class_property(c: &Cst, k: NodeId, name: &str) -> Option<Decl> {
    let mut cur = Some(k);
    let mut guard = 0;
    while let Some(x) = cur {
        if let Some(d) = find_in_items(c, x, name, DeclKind::Prop) {
            return Some(d);
        }
        guard += 1;
        if guard > 32 {
            break;
        }
        cur = base_class(c, x);
    }
    None
}

/// What `name`, used at node `at`, refers to.
pub(crate) fn resolve(c: &Cst, at: NodeId, name: &str) -> Option<Decl> {
    let mut cur = c.parent(at);
    while let Some(n) = cur {
        match c.kind(n) {
            SeqBlock | ParBlock | Body => {
                if let Some(d) = find_in_items(c, n, name, DeclKind::Local) {
                    return Some(d);
                }
            }
            LoopStmt => {
                if let Some(d) = find_in_items(c, n, name, DeclKind::Local) {
                    return Some(d);
                }
                if c.keyword(n).is_some_and(|t| c.text_of(t) == "foreach")
                    && c.toks(n).skip_while(|&t| c.text_of(t) != "[").any(|t| c.text_of(t) == name)
                {
                    return Some(Decl { kind: DeclKind::Local, item: n, dtype: None });
                }
            }
            FunctionDecl | TaskDecl => {
                if let Some(list) = c.tree.child(n, PortList) {
                    for p in kids(c, list) {
                        if c.kind(p) == Port {
                            if let Some(d) = declarators_named(c, p, name) {
                                return Some(Decl { kind: DeclKind::Arg, item: d, dtype: c.tree.child(p, DataType) });
                            }
                        }
                    }
                }
                // `function T C::f`: the body sees C's members.
                if let Some(nm) = c.tree.child(n, Name) {
                    let toks = c.tree.token_range(nm);
                    if toks.len() >= 3 && c.text_of(toks[toks.len() - 2]) == "::" {
                        if let Some(k) = class_named(c, c.text_of(toks[toks.len() - 3])) {
                            if let Some(d) = class_property(c, k, name) {
                                return Some(d);
                            }
                        }
                    }
                }
            }
            ClassDecl => {
                if let Some(d) = class_property(c, n, name) {
                    return Some(d);
                }
            }
            ModuleDecl | InterfaceDecl | ProgramDecl | PackageDecl | SourceFile => {
                if let Some(d) = find_in_items(c, n, name, DeclKind::Other) {
                    return Some(d);
                }
                if let Some(list) = c.tree.child(n, Header).and_then(|h| c.tree.child(h, PortList)) {
                    for p in kids(c, list) {
                        if c.kind(p) == Port {
                            if let Some(d) = declarators_named(c, p, name) {
                                return Some(Decl { kind: DeclKind::Other, item: d, dtype: c.tree.child(p, DataType) });
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        cur = c.parent(n);
    }
    None
}

/// The type name a data type spells (`pkt`, `p::pkt`, `pkt#(8)`), through
/// one typedef in the tree.
pub(crate) fn type_head<'t>(c: &Cst<'t>, dt: NodeId) -> Option<&'t str> {
    let toks = c.tree.token_range(dt);
    let end = toks.iter().position(|&t| c.text_of(t) == "#").unwrap_or(toks.len());
    let head = *toks[..end].last()?;
    is_ident(c, head).then(|| c.text_of(head))
}

pub(crate) fn typedef_named(c: &Cst, name: &str) -> Option<NodeId> {
    c.first_named(TypedefDecl, name)
}

/// The declared type is a class handle.
pub(crate) fn is_class_type(c: &Cst, d: &Decl) -> bool {
    let Some(dt) = d.dtype else { return false };
    if c.toks(dt).any(|t| c.text_of(t) == "virtual") || c.kind(d.item) != Declarator {
        return false;
    }
    if c.tree.child(d.item, Dim).is_some() {
        return false;
    }
    let mut name = match type_head(c, dt) {
        Some(n) => n,
        None => return false,
    };
    for _ in 0..4 {
        if class_named(c, name).is_some() {
            return true;
        }
        let Some(t) = typedef_named(c, name) else { return false };
        if c.tree.child(t, Dim).is_some() {
            return false;
        }
        match c.tree.child(t, DataType).and_then(|x| type_head(c, x)) {
            Some(n) if n != name => name = n,
            _ => return false,
        }
    }
    false
}

/// `getSymbolReference()` of an assignment's left side.
pub(crate) fn symbol_ref(c: &Cst, n: NodeId) -> Option<(Decl, String)> {
    let n = unparen(c, n);
    match c.kind(n) {
        NameExpr => {
            let t = c.first_tok(n)?;
            if !is_ident(c, t) {
                return None;
            }
            let name = c.text_of(t);
            resolve(c, n, name).map(|d| (d, name.to_string()))
        }
        SelectExpr => symbol_ref(c, kids(c, n).first().copied()?),
        MemberExpr => {
            let base = kids(c, n).first().copied()?;
            let member = last_tok(c, n)?;
            let mname = c.text_of(member).to_string();
            let b = unparen(c, base);
            if c.kind(b) == NameExpr && c.first_tok(b).is_some_and(|t| matches!(c.text_of(t), "this" | "super")) {
                let mut cur = c.parent(n);
                while let Some(x) = cur {
                    if c.kind(x) == ClassDecl {
                        let start = if c.text_of(c.first_tok(b)?) == "super" { base_class(c, x)? } else { x };
                        return class_property(c, start, &mname).map(|d| (d, mname));
                    }
                    if matches!(c.kind(x), FunctionDecl | TaskDecl) {
                        if let Some(nm) = c.tree.child(x, Name) {
                            let toks = c.tree.token_range(nm);
                            if toks.len() >= 3 && c.text_of(toks[toks.len() - 2]) == "::" {
                                let k = class_named(c, c.text_of(toks[toks.len() - 3]))?;
                                return class_property(c, k, &mname).map(|d| (d, mname));
                            }
                        }
                    }
                    cur = c.parent(x);
                }
                return None;
            }
            let (bd, _) = symbol_ref(c, b)?;
            if is_class_type(c, &bd) {
                // A member of a class handle: the property itself.
                let dt = bd.dtype?;
                let k = class_named(c, type_head(c, dt)?).or_else(|| {
                    let t = typedef_named(c, type_head(c, dt)?)?;
                    class_named(c, type_head(c, c.tree.child(t, DataType)?)?)
                })?;
                return class_property(c, k, &mname).map(|d| (d, mname));
            }
            None
        }
        _ => None,
    }
}

pub(crate) fn assignment_op<'t>(c: &Cst<'t>, n: NodeId) -> &'t str {
    if c.kind(n) != BinaryExpr {
        return "";
    }
    c.toks(n).next().map(|t| c.text_of(t)).unwrap_or("")
}

pub(crate) fn in_class(c: &Cst, n: NodeId) -> bool {
    let mut cur = c.parent(n);
    while let Some(x) = cur {
        if c.kind(x) == ClassDecl {
            return true;
        }
        cur = c.parent(x);
    }
    false
}
