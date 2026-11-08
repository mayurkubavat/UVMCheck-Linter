//! Class roles (component, object, sequence and so on), user classes and their
//! methods, for the methodology rules.
//!
//! Most methodology rules read slang's AST of method bodies with types
//! resolved. Here they are [`crate::Check::Mixed`] rules: the class model
//! gives roles, bases and member types, and the syntax trees give the
//! statements, with names resolved against the model's members, arguments
//! and locals (see [`Ctx`]). These are the C++ helpers of
//! `MethodologySupport.cpp` and the parts of `SweSupport.cpp` they use.

use std::collections::HashMap;

use dv_model::{Class, ClassId, Design, Loc, Method};
use sv_syntax::{Child, NodeId, SyntaxKind};

use crate::syntax::Cst;
use SyntaxKind::*;

// ---- roles (swe::Role) ----

/// What a class is for in a UVM testbench, from its nearest known uvm_* base.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    None,
    Object,
    Config,
    Item,
    Sequence,
    Component,
    Driver,
    Monitor,
    Sequencer,
    Agent,
    Scoreboard,
    Subscriber,
    Env,
    Test,
    RegModel,
    Catcher,
    Callback,
}

pub(crate) fn role_name(r: Role) -> &'static str {
    match r {
        Role::None => "class",
        Role::Object => "object",
        Role::Config => "config",
        Role::Item => "sequence item",
        Role::Sequence => "sequence",
        Role::Component => "component",
        Role::Driver => "driver",
        Role::Monitor => "monitor",
        Role::Sequencer => "sequencer",
        Role::Agent => "agent",
        Role::Scoreboard => "scoreboard",
        Role::Subscriber => "subscriber",
        Role::Env => "env",
        Role::Test => "test",
        Role::RegModel => "register model",
        Role::Catcher => "report catcher",
        Role::Callback => "callback",
    }
}

pub(crate) fn is_component_role(r: Role) -> bool {
    matches!(
        r,
        Role::Component
            | Role::Driver
            | Role::Monitor
            | Role::Sequencer
            | Role::Agent
            | Role::Scoreboard
            | Role::Subscriber
            | Role::Env
            | Role::Test
    )
}

fn says_config(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    l.contains("cfg") || l.contains("config")
}

/// `swe::classRole`: the first uvm_* ancestor whose name decides the role.
pub(crate) fn role(d: &Design, c: &Class) -> Role {
    for a in d.ancestors(c.id) {
        let n = a.name.as_str();
        if !n.starts_with("uvm_") {
            continue;
        }
        if n == "uvm_driver" || n == "uvm_push_driver" {
            return Role::Driver;
        }
        if n == "uvm_monitor" {
            return Role::Monitor;
        }
        if n.starts_with("uvm_sequencer") || n == "uvm_push_sequencer" {
            return Role::Sequencer;
        }
        if n == "uvm_agent" {
            return Role::Agent;
        }
        if n == "uvm_scoreboard" {
            return Role::Scoreboard;
        }
        if n == "uvm_subscriber" {
            return Role::Subscriber;
        }
        if n == "uvm_env" {
            return Role::Env;
        }
        if n == "uvm_test" {
            return Role::Test;
        }
        if n.starts_with("uvm_reg") || n == "uvm_mem" || n == "uvm_vreg" {
            return Role::RegModel;
        }
        if n == "uvm_report_catcher" {
            return Role::Catcher;
        }
        if n.starts_with("uvm_callback") {
            return Role::Callback;
        }
        if n == "uvm_sequence" || n == "uvm_sequence_base" || n == "uvm_reg_sequence" {
            return Role::Sequence;
        }
        if n == "uvm_sequence_item" || n == "uvm_transaction" {
            return Role::Item;
        }
        if n == "uvm_component" || n.starts_with("uvm_tlm") || n == "uvm_root" {
            return Role::Component;
        }
        if n == "uvm_object" || n == "uvm_void" {
            return if says_config(&c.name) { Role::Config } else { Role::Object };
        }
    }
    Role::None
}

/// `swe::uvmBaseName`: the nearest uvm_* ancestor's name.
pub(crate) fn uvm_base_name<'d>(d: &'d Design, c: &Class) -> &'d str {
    d.ancestors(c.id).find(|a| a.name.starts_with("uvm_")).map(|a| a.name.as_str()).unwrap_or("")
}

/// `swe::isLibrarySymbol`: in uvm_pkg or a uvm_* class, or written in the
/// test stub or uvm_pkg.sv.
pub(crate) fn is_library(d: &Design, c: &Class) -> bool {
    if c.name.starts_with("uvm_") || c.scope.split("::").any(|s| s.starts_with("uvm_")) {
        return true;
    }
    let f = d.file_name(c.loc);
    f.contains("uvm_mock.svh") || f.contains("uvm_pkg.sv")
}

/// `swe::userClasses`: every class that is not the library's.
pub(crate) fn user_classes(d: &Design) -> impl Iterator<Item = &Class> {
    d.classes().iter().filter(move |c| !is_library(d, c))
}

/// `swe::classMethods`: methods with bodies (not built-in or pure).
pub(crate) fn class_methods(c: &Class) -> impl Iterator<Item = &Method> {
    c.methods.iter().filter(|m| !m.is_builtin && !m.is_pure)
}

/// The class a type string names, looking through unpacked dimensions and
/// queues (`my_drv$[0:3]`, `pk::pc#(8)`); a class in `near`'s scope wins.
pub(crate) fn class_of_type<'d>(d: &'d Design, ty: &str, near: Option<&Class>) -> Option<&'d Class> {
    class_of_type_in(d, None, ty, near)
}

/// Typedef names and the type text they stand for (`$` marks unpacked
/// dimensions), from every tree.
pub(crate) type Typedefs = HashMap<String, String>;

/// [`class_of_type`], also through typedefs.
pub(crate) fn class_of_type_in<'d>(
    d: &'d Design,
    td: Option<&Typedefs>,
    ty: &str,
    near: Option<&Class>,
) -> Option<&'d Class> {
    let mut ty = ty.to_string();
    for _ in 0..8 {
        let base = ty.split('$').next().unwrap_or("");
        let base = base.split('#').next().unwrap_or("").trim();
        let name = base.rsplit("::").next().unwrap_or(base);
        if d.classes().iter().any(|c| c.name == name) {
            break;
        }
        match td.and_then(|t| t.get(name)) {
            Some(next) if *next != ty => ty = next.clone(),
            _ => break,
        }
    }
    let base = ty.split('$').next().unwrap_or("");
    let base = base.split('#').next().unwrap_or("").trim();
    let name = base.rsplit("::").next().unwrap_or(base);
    if name.is_empty() {
        return None;
    }
    let mut best = None;
    for c in d.classes().iter().filter(|c| c.name == name) {
        if near.is_some_and(|n| n.scope == c.scope) {
            return Some(c);
        }
        best = best.or(Some(c));
    }
    best
}

/// A property's class, through arrays and queues of handles.
pub(crate) fn property_class<'d>(d: &'d Design, owner: &Class, p: &dv_model::Property) -> Option<&'d Class> {
    match p.type_class {
        Some(id) => Some(d.class(id)),
        None if p.ty.contains('$') => class_of_type(d, &p.ty, Some(owner)),
        None => None,
    }
}

/// `swe::derivesFrom(cls, base)` (strictly below).
pub(crate) fn derives(d: &Design, c: ClassId, base: ClassId) -> bool {
    d.ancestors(c).any(|a| a.id == base)
}

/// A location as a map key.
pub(crate) type LocKey = (u32, u32, u32, Option<(u32, u32)>);

pub(crate) fn key(l: Loc) -> LocKey {
    (l.file, l.line, l.column, l.expansion)
}

// ---- the trees ----

/// Class and subroutine declarations of every tree, by where their name is
/// (the location the model gives classes and methods).
pub(crate) struct Index<'a, 't> {
    pub csts: &'a [Cst<'t>],
    decls: HashMap<LocKey, (usize, NodeId)>,
    pub typedefs: Typedefs,
}

impl<'a, 't> Index<'a, 't> {
    pub fn new(csts: &'a [Cst<'t>]) -> Self {
        let mut decls = HashMap::new();
        let mut typedefs = Typedefs::new();
        for c in csts {
            for n in c.nodes(TypedefDecl) {
                let (Some(ty), Some(name)) = (c.tree.child(n, DataType), c.tree.child(n, Name)) else { continue };
                let mut t = c.tree.bare_text(ty);
                if c.tree.child(n, Dim).is_some() {
                    t.push_str("$[]");
                }
                typedefs.entry(c.tree.bare_text(name)).or_insert(t);
            }
        }
        for (i, c) in csts.iter().enumerate() {
            for n in c.nodes_of(&[ClassDecl, FunctionDecl, TaskDecl]) {
                let Some(name) = c.tree.child(n, Name) else { continue };
                let Some((_, last)) = c.tree.token_bounds(name) else { continue };
                decls.entry(key(c.loc(last))).or_insert((i, n));
            }
        }
        Index { csts, decls, typedefs }
    }

    pub fn decl(&self, l: Loc) -> Option<(&'a Cst<'t>, NodeId)> {
        self.decls.get(&key(l)).map(|&(i, n)| (&self.csts[i], n))
    }

    /// A method's body (`Body` node), with its tree.
    pub fn body(&self, m: &Method) -> Option<(&'a Cst<'t>, NodeId)> {
        let (c, n) = self.decl(m.loc)?;
        if !matches!(c.kind(n), FunctionDecl | TaskDecl) {
            return None;
        }
        Some((c, c.tree.child(n, Body)?))
    }
}

impl Cst<'_> {
    /// Nodes of any of these kinds, in source order.
    pub(crate) fn nodes_of(&self, kinds: &[SyntaxKind]) -> Vec<NodeId> {
        let mut v = Vec::new();
        for &k in kinds {
            v.extend(self.nodes(k));
        }
        v.sort_by_key(|n| self.first_tok(*n).unwrap_or(u32::MAX));
        v
    }

    /// Every node under `n` (itself included), in pre-order.
    pub(crate) fn subtree(&self, n: NodeId) -> Vec<NodeId> {
        self.tree.subtree(n)
    }

    pub(crate) fn node_kids(&self, n: NodeId) -> Vec<NodeId> {
        self.tree.children(n).collect()
    }

    /// Where a node starts.
    pub(crate) fn loc_of(&self, n: NodeId) -> Loc {
        self.first_tok(n).map(|t| self.loc(t)).unwrap_or_default()
    }

    /// The node's tokens joined with single spaces where the source had
    /// trivia: slang's `exprText` of the written expression.
    pub(crate) fn expr_text(&self, n: NodeId) -> String {
        let mut s = String::new();
        for (i, t) in self.tree.token_range(n).into_iter().enumerate() {
            let tok = &self.tree.tokens[t as usize];
            if i > 0 && tok.space_before {
                s.push(' ');
            }
            s.push_str(&tok.text);
        }
        s
    }

    /// A call's arguments as slang counts them: one per comma-separated
    /// slot, `None` for an empty one; none at all for `()`.
    pub(crate) fn call_args(&self, call: NodeId) -> Vec<Option<NodeId>> {
        let Some(list) = self.tree.child(call, ArgList) else { return Vec::new() };
        let mut out = vec![None];
        for k in self.kids(list) {
            match *k {
                Child::Token(t) if self.text_of(t) == "," => out.push(None),
                Child::Node(a) => *out.last_mut().unwrap() = Some(a),
                _ => {}
            }
        }
        if out.len() == 1 && out[0].is_none() {
            out.clear();
        }
        out
    }

    /// An ordered argument's expression (`None` for a named or empty one).
    pub(crate) fn arg_expr(&self, a: Option<NodeId>) -> Option<NodeId> {
        let a = a?;
        if self.kind(a) != Arg || self.tok(a, ".").is_some() {
            return None;
        }
        self.tree.children(a).next()
    }

    /// The called name: `create` of `T::type_id::create(...)`, `get` of
    /// `cfg.get()`.
    pub(crate) fn callee_name(&self, call: NodeId) -> &str {
        self.tree.children(call).next().map(|n| self.last_text(n)).unwrap_or("")
    }

    /// Strips parentheses (slang's conversions and parens do not show).
    pub(crate) fn strip(&self, mut n: NodeId) -> NodeId {
        while self.kind(n) == ParenExpr {
            match self.tree.children(n).next() {
                Some(x) => n = x,
                None => break,
            }
        }
        n
    }

    /// A string literal's value, without the quotes.
    pub(crate) fn string_value(&self, n: NodeId) -> Option<String> {
        let n = self.strip(n);
        if self.kind(n) != Literal {
            return None;
        }
        let t = self.tree.token_range(n);
        if t.len() != 1 {
            return None;
        }
        let s = self.text_of(t[0]);
        (s.len() >= 2 && s.starts_with('"') && s.ends_with('"')).then(|| s[1..s.len() - 1].to_string())
    }

    pub(crate) fn is_null(&self, n: NodeId) -> bool {
        let n = self.strip(n);
        self.kind(n) == NameExpr && self.tree.bare_text(n) == "null"
    }
}

// ---- names in a method body ----

/// Where a body is: its class and method, to resolve names to classes.
pub(crate) struct Ctx<'d, 'x> {
    pub d: &'d Design,
    pub cls: &'d Class,
    pub m: Option<&'d Method>,
    pub td: Option<&'x Typedefs>,
}

impl<'d> Ctx<'d, '_> {
    pub fn class_of_type(&self, ty: &str) -> Option<&'d Class> {
        class_of_type_in(self.d, self.td, ty, Some(self.cls))
    }

    /// The class of a name visible in the body: a local, an argument, or a
    /// property of the class or a base.
    pub fn class_of_name(&self, name: &str) -> Option<&'d Class> {
        if name == "this" {
            return Some(self.cls);
        }
        if let Some(m) = self.m {
            if let Some(l) = m.locals.iter().find(|l| l.name == name) {
                return self.class_of_type(&l.ty);
            }
            if let Some(a) = m.arguments.iter().find(|a| a.name == name) {
                return match a.type_class {
                    Some(id) => Some(self.d.class(id)),
                    None => self.class_of_type(&a.ty),
                };
            }
        }
        self.member_class(self.cls, name)
    }

    /// The class of property `name` of `c` or a base.
    pub fn member_class(&self, c: &'d Class, name: &str) -> Option<&'d Class> {
        std::iter::once(c)
            .chain(self.d.ancestors(c.id))
            .find_map(|k| k.properties.iter().find(|p| p.name == name).map(|p| (k, p)))
            .and_then(|(k, p)| property_class(self.d, k, p))
    }

    /// The class of an expression: names, members, selects and `this`.
    pub fn class_of(&self, c: &Cst, e: NodeId) -> Option<&'d Class> {
        let e = c.strip(e);
        match c.kind(e) {
            NameExpr => {
                if c.tree.children(e).next().is_some() {
                    return None;
                }
                self.class_of_name(&c.tree.bare_text(e))
            }
            SelectExpr => self.class_of(c, c.tree.children(e).next()?),
            MemberExpr => {
                let obj = c.tree.children(e).next()?;
                let member = c.last_text(e);
                let oc = if c.kind(obj) == NameExpr && c.tree.bare_text(obj) == "super" {
                    self.cls.base.map(|b| self.d.class(b))?
                } else {
                    self.class_of(c, obj)?
                };
                self.member_class(oc, member)
            }
            _ => None,
        }
    }
}
