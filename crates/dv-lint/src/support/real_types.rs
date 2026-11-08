//! Expression types from the declarations the syntax names, for the
//! real-number checks (SV-BUG-005, SV-BUG-006). An expression whose type the
//! syntax does not give is left alone.

use std::collections::HashMap;

use sv_syntax::NodeId;

use crate::support::class_items::{class_items, declarators, kids, op, qualifiers, user_code};

use crate::support::type_text::base_name;

use crate::support::constraints::class_names;



use crate::syntax::Cst;



use sv_syntax::SyntaxKind::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Ty {
    Real,
    Int,
}

pub(crate) const REAL_SYS: &[&str] = &[
    "$realtime", "$itor", "$bitstoreal", "$shortrealtobits", "$bitstoshortreal", "$sqrt", "$ln", "$log10", "$exp",
    "$pow", "$floor", "$ceil", "$sin", "$cos", "$tan", "$asin", "$acos", "$atan", "$atan2", "$hypot", "$sinh",
    "$cosh", "$tanh", "$asinh", "$acosh", "$atanh", "$dist_normal_real",
];

pub(crate) const INT_SYS: &[&str] = &[
    "$time", "$stime", "$rtoi", "$realtobits", "$clog2", "$bits", "$size", "$urandom", "$random", "$urandom_range",
    "$countones", "$onehot", "$onehot0", "$isunknown", "$high", "$low", "$left", "$right", "$increment", "$dimensions",
];

pub(crate) struct Types<'a> {
    pub(crate) c: &'a Cst<'a>,
    pub(crate) classes: HashMap<String, NodeId>,
    pub(crate) typedefs: HashMap<String, NodeId>,
    /// Functions by name: their return data type.
    pub(crate) functions: HashMap<String, Option<NodeId>>,
}

impl<'a> Types<'a> {
    pub(crate) fn new(c: &'a Cst<'a>) -> Self {
        let mut typedefs = HashMap::new();
        for t in c.nodes(TypedefDecl) {
            if let (Some(dt), Some(n)) = (c.tree.child(t, DataType), c.tree.child(t, Name)) {
                if !c.tree.children(t).any(|x| c.kind(x) == Dim) {
                    typedefs.insert(c.tree.bare_text(n), dt);
                }
            }
        }
        let mut functions: HashMap<String, Option<NodeId>> = HashMap::new();
        for f in c.nodes(FunctionDecl) {
            let Some(nm) = c.tree.child(f, Name) else { continue };
            let rt = c.tree.child(f, ReturnType).and_then(|r| c.tree.child(r, DataType));
            let name = c.last_text(nm).to_string();
            // Two functions of one name with different types: unknown.
            match functions.get(&name) {
                Some(prev) if prev.map(|d| self_ty(c, d, &typedefs)) != rt.map(|d| self_ty(c, d, &typedefs)) => {
                    functions.insert(name, None);
                }
                Some(_) => {}
                None => {
                    functions.insert(name, rt);
                }
            }
        }
        Types { c, classes: class_names(c), typedefs, functions }
    }

    pub(crate) fn of_type(&self, dt: NodeId) -> Option<Ty> {
        self_ty(self.c, dt, &self.typedefs)
    }

    /// The data type a declaration of `name` visible at `at` gives.
    pub(crate) fn decl_type(&self, at: NodeId, name: &str) -> Option<NodeId> {
        let c = self.c;
        let mut x = at;
        while let Some(p) = c.parent(x) {
            match c.kind(p) {
                Body | SeqBlock | ParBlock | ModuleDecl | PackageDecl | ProgramDecl | InterfaceDecl | SourceFile => {
                    if let Some(r) = find_in(c, c.tree.children(p), name) {
                        return r;
                    }
                }
                FunctionDecl | TaskDecl => {
                    if let Some(r) = self.port_type(p, name) {
                        return r;
                    }
                    if let Some(cls) = self.out_of_block_class(p) {
                        return self.prop_type(cls, name)?;
                    }
                }
                ClassDecl => return self.prop_type(p, name)?,
                _ => {}
            }
            x = p;
        }
        None
    }

    /// A port of a routine: `Some(type)` when declared (`Some(None)` for an
    /// untyped port), carrying the previous port's type over.
    pub(crate) fn port_type(&self, f: NodeId, name: &str) -> Option<Option<NodeId>> {
        let c = self.c;
        let pl = c.tree.child(f, PortList)?;
        let mut last: Option<NodeId> = None;
        for p in c.tree.children(pl).filter(|&x| c.kind(x) == Port) {
            if let Some(dt) = c.tree.child(p, DataType) {
                last = Some(dt);
            } else if c.toks(p).next().is_some() {
                // A direction without a type starts a new (logic) port.
                last = None;
            }
            for (dcl, v, _) in declarators(c, p) {
                if v == name {
                    return Some(if c.tree.children(dcl).any(|k| c.kind(k) == Dim) { None } else { last });
                }
            }
        }
        None
    }

    pub(crate) fn out_of_block_class(&self, f: NodeId) -> Option<NodeId> {
        let c = self.c;
        let toks = c.tree.token_range(c.tree.child(f, Name)?);
        if toks.len() == 3 && c.text_of(toks[1]) == "::" {
            return self.classes.get(c.text_of(toks[0])).copied();
        }
        None
    }

    /// A property of `cls` or its bases: `Some(type)` when found.
    pub(crate) fn prop_type(&self, cls: NodeId, name: &str) -> Option<Option<NodeId>> {
        let c = self.c;
        let mut cur = Some(cls);
        let mut guard = 0;
        while let Some(k) = cur {
            if let Some(r) = find_in(c, class_items(c, k).into_iter(), name) {
                return Some(r);
            }
            guard += 1;
            cur = if guard < 64 { base_name(c, k).and_then(|b| self.classes.get(&b).copied()) } else { None };
        }
        None
    }

    /// The class a handle expression names.
    pub(crate) fn class_of(&self, e: NodeId) -> Option<NodeId> {
        let c = self.c;
        if c.kind(e) != NameExpr {
            return None;
        }
        let name = c.tree.bare_text(e);
        if name == "this" {
            let mut x = e;
            while let Some(p) = c.parent(x) {
                match c.kind(p) {
                    ClassDecl => return Some(p),
                    FunctionDecl | TaskDecl => {
                        if let Some(k) = self.out_of_block_class(p) {
                            return Some(k);
                        }
                    }
                    _ => {}
                }
                x = p;
            }
            return None;
        }
        let dt = self.decl_type(e, &name)?;
        let n = crate::support::type_text::type_name(c, dt)?;
        self.classes.get(&n).copied()
    }

    pub(crate) fn expr(&self, e: NodeId) -> Option<Ty> {
        let c = self.c;
        let k = kids(c, e);
        match c.kind(e) {
            ParenExpr => self.expr(*k.first()?),
            Literal => {
                let t = c.tree.bare_text(e);
                let f = t.chars().next()?;
                if f == '"' {
                    return None;
                }
                if t.contains('\'') || !f.is_ascii_digit() {
                    return Some(Ty::Int);
                }
                if ["fs", "ps", "ns", "us", "ms", "s", "step"].iter().any(|u| t.ends_with(u)) {
                    return Some(Ty::Real);
                }
                if t.contains(['.', 'e', 'E']) {
                    return Some(Ty::Real);
                }
                Some(Ty::Int)
            }
            NameExpr => {
                let name = c.tree.bare_text(e);
                if name.starts_with('$') {
                    return sys_ty(&name);
                }
                self.of_type(self.decl_type(e, &name)?)
            }
            MemberExpr => {
                let base = *k.first()?;
                let cls = self.class_of(base)?;
                self.of_type(self.prop_type(cls, c.last_text(e))??)
            }
            CallExpr => {
                let callee = *k.first()?;
                let name = c.tree.bare_text(callee);
                if name.starts_with('$') {
                    return sys_ty(&name);
                }
                if c.kind(callee) != NameExpr {
                    return None;
                }
                self.of_type(self.functions.get(&name).copied()??)
            }
            CastExpr => {
                let t = c.tree.bare_text(*k.first()?);
                match t.as_str() {
                    "real" | "shortreal" | "realtime" => Some(Ty::Real),
                    "signed" | "unsigned" => self.expr(*k.get(1)?),
                    "int" | "integer" | "bit" | "logic" | "byte" | "shortint" | "longint" | "time" => Some(Ty::Int),
                    _ if t.chars().all(|x| x.is_ascii_digit()) => Some(Ty::Int),
                    _ => self.of_type(*self.typedefs.get(&t)?),
                }
            }
            PrefixExpr => match c.text_of(c.first_tok(e)?) {
                "-" | "+" => self.expr(*k.first()?),
                "!" | "~" | "&" | "|" | "^" | "~&" | "~|" | "~^" | "^~" => Some(Ty::Int),
                _ => None,
            },
            BinaryExpr => match op(c, e) {
                "+" | "-" | "*" | "/" | "**" => {
                    let (a, b) = (self.expr(*k.first()?), self.expr(*k.get(1)?));
                    match (a, b) {
                        (Some(Ty::Real), _) | (_, Some(Ty::Real)) => Some(Ty::Real),
                        (Some(Ty::Int), Some(Ty::Int)) => Some(Ty::Int),
                        _ => None,
                    }
                }
                "%" | "&" | "|" | "^" | "~^" | "^~" | "<<" | ">>" | "<<<" | ">>>" | "==" | "!=" | "===" | "!=="
                | "<" | ">" | ">=" | "&&" | "||" => Some(Ty::Int),
                _ => None,
            },
            CondExpr => {
                let (a, b) = (self.expr(*k.get(1)?), self.expr(*k.get(2)?));
                match (a, b) {
                    (Some(Ty::Real), _) | (_, Some(Ty::Real)) => Some(Ty::Real),
                    (Some(Ty::Int), Some(Ty::Int)) => Some(Ty::Int),
                    _ => None,
                }
            }
            SelectExpr => match self.expr(*k.first()?) {
                Some(Ty::Int) => Some(Ty::Int),
                _ => None,
            },
            _ => None,
        }
    }
}

pub(crate) fn sys_ty(name: &str) -> Option<Ty> {
    if REAL_SYS.contains(&name) {
        Some(Ty::Real)
    } else if INT_SYS.contains(&name) {
        Some(Ty::Int)
    } else {
        None
    }
}

/// The type a data type names, through typedefs.
pub(crate) fn self_ty(c: &Cst, dt: NodeId, typedefs: &HashMap<String, NodeId>) -> Option<Ty> {
    let mut dt = dt;
    for _ in 0..8 {
        let ft = c.first_tok(dt)?;
        let first = c.text_of(ft);
        return match first {
            "real" | "shortreal" | "realtime" => Some(Ty::Real),
            "bit" | "logic" | "reg" | "int" | "integer" | "byte" | "shortint" | "longint" | "time" | "enum" => {
                Some(Ty::Int)
            }
            "struct" | "union" if c.tok(dt, "packed").is_some() => Some(Ty::Int),
            _ if c.tree.tokens[ft as usize].kind == sv_syntax::TokenKind::Identifier
                && c.tree.token_range(dt).len() == 1 =>
            {
                dt = *typedefs.get(first)?;
                continue;
            }
            _ => None,
        };
    }
    None
}

/// The declaration of `name` among `items`: `Some(type)` when found (`None`
/// inside for an unpacked array or an untyped one).
pub(crate) fn find_in(c: &Cst, items: impl Iterator<Item = NodeId>, name: &str) -> Option<Option<NodeId>> {
    for d in items {
        if !matches!(c.kind(d), DataDecl | ParamDecl) || qualifiers(c, d).contains(&"typedef") {
            continue;
        }
        for (dcl, v, _) in declarators(c, d) {
            if v == name {
                if c.tree.children(dcl).any(|k| c.kind(k) == Dim) {
                    return Some(None);
                }
                return Some(c.tree.child(d, DataType));
            }
        }
    }
    None
}

impl Types<'_> {
    /// A routine named `name` in `cls` or its bases (declaration or
    /// prototype), not one of the library's.
    pub(crate) fn method(&self, cls: NodeId, name: &str) -> Option<NodeId> {
        let c = self.c;
        let mut cur = Some(cls);
        let mut guard = 0;
        while let Some(k) = cur {
            if crate::support::class_items::decl_name(c, k).is_some_and(|(n, _)| n.starts_with("uvm_")) || !user_code(c, k) {
                return None;
            }
            for m in class_items(c, k) {
                if matches!(c.kind(m), FunctionDecl | TaskDecl | MethodPrototype)
                    && c.tree.child(m, Name).is_some_and(|n| c.last_text(n) == name)
                {
                    return Some(m);
                }
            }
            guard += 1;
            cur = if guard < 64 { base_name(c, k).and_then(|b| self.classes.get(&b).copied()) } else { None };
        }
        None
    }

    /// The class around `n` (also for an out-of-block body).
    pub(crate) fn enclosing_class(&self, n: NodeId) -> Option<NodeId> {
        let c = self.c;
        let mut x = n;
        while let Some(p) = c.parent(x) {
            match c.kind(p) {
                ClassDecl => return Some(p),
                FunctionDecl | TaskDecl => {
                    if let Some(k) = self.out_of_block_class(p) {
                        return Some(k);
                    }
                }
                _ => {}
            }
            x = p;
        }
        None
    }

    /// The routine a call names, when the syntax resolves it.
    pub(crate) fn callee(&self, call: NodeId, free: &HashMap<String, Vec<NodeId>>) -> Option<NodeId> {
        let c = self.c;
        let callee = c.tree.children(call).next()?;
        match c.kind(callee) {
            NameExpr => {
                let name = c.tree.bare_text(callee);
                if let Some(m) = self.enclosing_class(call).and_then(|k| self.method(k, &name)) {
                    return Some(m);
                }
                match free.get(&name).map(Vec::as_slice) {
                    Some([f]) if user_code(c, *f) => Some(*f),
                    _ => None,
                }
            }
            MemberExpr => {
                let base = c.tree.children(callee).next()?;
                let cls = if c.kind(base) == NameExpr && c.tree.bare_text(base) == "super" {
                    let k = self.enclosing_class(call)?;
                    self.classes.get(&base_name(c, k)?).copied()?
                } else {
                    self.class_of(base)?
                };
                self.method(cls, c.last_text(callee))
            }
            _ => None,
        }
    }
}

impl Types<'_> {
    /// The declared type text of a variable or member argument.
    pub(crate) fn arg_type(&self, e: NodeId) -> Option<String> {
        let c = self.c;
        let dt = match c.kind(e) {
            NameExpr => self.decl_type(e, &c.tree.bare_text(e))?,
            MemberExpr => {
                let cls = self.class_of(c.tree.children(e).next()?)?;
                self.prop_type(cls, c.last_text(e))??
            }
            _ => return None,
        };
        Some(c.tree.bare_text(dt))
    }
}
