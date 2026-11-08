//! Method bodies: calls (with what they call), assignments, timing controls
//! and `super.new`, as slang's model reports them from the elaborated AST.

use std::collections::HashMap;

use dv_model::{Assignment, Call, Direction, Loc, RootKind, TypeShape};
use sv_syntax::SyntaxKind;
use SyntaxKind::*;

use super::{Builder, Env, Found, Member, NodeRef, Resolved, ScopeKind};

/// The type of an expression, as far as calls need it.
#[derive(Clone)]
pub(crate) enum Ty {
    Class(usize, Env),
    /// A struct or union: its DataType and the scope it is written in.
    Struct(NodeRef, usize),
    Std(&'static str),
    Array {
        elem: Box<Ty>,
        /// The index type of an associative array.
        index: Option<Box<Ty>>,
    },
    Str,
    Enum,
    Event,
    Vif,
    Other,
    Unknown,
    /// A named type that is declared nowhere.
    Error,
    /// An embedded covergroup's instance.
    Covergroup,
}

/// What a call calls.
struct Callee {
    system: bool,
    scope: String,
    declared_virtual: bool,
    builtin: bool,
    ret: Ty,
    /// Formal arguments: direction and default value.
    formals: Vec<(Direction, Option<NodeRef>, String)>,
    /// A system call on a value (`q.size()`): the value is argument 0.
    receiver_is_arg: bool,
    /// System calls: positions written by the call (`$cast`'s first).
    written: Written,
    /// Where defaults are evaluated: class, its type arguments, scope.
    ctx: Option<(Option<usize>, Env, usize)>,
}

#[derive(Clone, Copy, PartialEq)]
enum Written {
    None,
    At(usize),
    From(usize),
}

enum Val {
    Var(Ty, RootKind),
    Sub(Callee),
}

/// What a body walk collects.
#[derive(Default)]
pub(crate) struct BodyFacts {
    pub calls: Vec<Call>,
    pub assignments: Vec<Assignment>,
    pub timing: Vec<Loc>,
    pub calls_super_new: bool,
    pub super_new: Option<(u32, Loc)>,
    /// A statement slang rejects: it then has no body, only the timing
    /// controls it finds in the syntax.
    pub bad: bool,
}

struct Walk {
    own_name: String,
    /// In a function (not a task): timing controls are errors.
    in_function: bool,
    class: usize,
    env: Env,
    scope: usize,
    frames: Vec<HashMap<String, (Ty, RootKind)>>,
    ret: Ty,
    facts: BodyFacts,
}

impl Walk {
    /// The same place with no locals.
    fn shallow(&self) -> Walk {
        Walk {
            own_name: String::new(),
            in_function: false,
            class: self.class,
            env: self.env.clone(),
            scope: self.scope,
            frames: Vec::new(),
            ret: Ty::Other,
            facts: BodyFacts::default(),
        }
    }
}

/// Methods every class has; calls to them are built-in methods of the class.
const CLASS_BUILTINS: &[&str] = &[
    "pre_randomize",
    "post_randomize",
    "get_randstate",
    "set_randstate",
    "srandom",
    "rand_mode",
    "constraint_mode",
];

const ASSIGN_OPS: &[&str] = &[
    "=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=", "<<<=", ">>>=",
];

fn is_expr(k: SyntaxKind) -> bool {
    matches!(
        k,
        BinaryExpr
            | PrefixExpr
            | PostfixExpr
            | CondExpr
            | InsideExpr
            | ParenExpr
            | Literal
            | NameExpr
            | ScopedExpr
            | MemberExpr
            | SelectExpr
            | CallExpr
            | ConcatExpr
            | AssignPattern
            | CastExpr
            | NewExpr
            | TaggedExpr
            | MatchExpr
            | TimedExpr
            | TypeExpr
    )
}

impl<'t> Builder<'t> {
    /// Calls, assignments and timing of a method body (`body` holds the
    /// ports and the Body node) of class `c` seen with `env`.
    pub(crate) fn body_facts(
        &mut self,
        decl: NodeRef,
        body: NodeRef,
        c: usize,
        env: &Env,
        name: &str,
    ) -> BodyFacts {
        let Some(b) = self.child(body, Body) else {
            return BodyFacts::default();
        };
        let scope = self.classes[c].scope;
        let mut args = HashMap::new();
        let ret = self
            .child(decl, ReturnType)
            .and_then(|r| self.child(r, DataType))
            .map_or(Ty::Other, |dt| self.decl_ty(Some(dt), None, scope, env));
        if self.own_tokens(decl).contains(&"function") && name != "new" {
            // The function's name is its return variable.
            args.insert(name.to_string(), (ret.clone(), RootKind::Local));
        }
        for (n, ty) in self.port_types(body, scope, env) {
            args.insert(n, (ty, RootKind::Argument));
        }
        let mut w = Walk {
            own_name: name.to_string(),
            in_function: self.own_tokens(decl).contains(&"function"),
            class: c,
            env: env.clone(),
            scope,
            frames: vec![args, HashMap::new()],
            ret,
            facts: BodyFacts::default(),
        };
        for s in self.children(b) {
            self.stmt(&mut w, s);
        }
        self.super_new_index(b, &mut w.facts);
        let mut facts = w.facts;
        if facts.bad {
            facts.calls.clear();
            facts.assignments.clear();
            facts.calls_super_new = false;
            facts.super_new = None;
        }
        facts
    }

    /// Marks the body as one slang rejects.
    fn poison(&self, w: &mut Walk, why: &str) {
        if !w.facts.bad && std::env::var_os("SV_SEMA_BAD").is_some() {
            eprintln!(
                "bad body {}::{}: {why}",
                self.classes[w.class].name, w.own_name
            );
        }
        w.facts.bad = true;
    }

    /// Whether an expression holds a name of an undeclared type, which
    /// makes slang reject it.
    fn has_error_name(&self, w: &Walk, n: NodeRef) -> bool {
        match self.kind(n) {
            NameExpr => return matches!(self.resolve_name(w, n), Some(Val::Var(Ty::Error, _))),
            ConstraintBlock | WithClause | DataType => return false,
            _ => {}
        }
        self.children(n)
            .into_iter()
            .any(|c| self.has_error_name(w, c))
    }

    /// Whether a constructor call is one slang rejects (the `new` itself
    /// stays): too many arguments, or one of an undeclared type.
    fn ctor_bad(&self, w: &Walk, c: &Callee, args: Option<NodeRef>) -> bool {
        let Some(al) = args else {
            return false;
        };
        let actuals: Vec<NodeRef> = self
            .children(al)
            .into_iter()
            .filter(|a| self.kind(*a) == Arg)
            .collect();
        let named = actuals
            .iter()
            .any(|a| self.own_tokens(*a).first() == Some(&"."));
        (!named && !c.builtin && actuals.len() > c.formals.len())
            || actuals.iter().any(|a| self.has_error_name(w, *a))
    }

    fn is_virtual_class(&self, c: usize) -> bool {
        let node = self.classes[c].node;
        self.tokens(node)
            .iter()
            .map(|&t| self.text(node.tree, t))
            .take_while(|t| *t != "class")
            .any(|t| t == "virtual")
    }

    fn is_assign_op(&self, n: NodeRef) -> bool {
        self.kind(n) == BinaryExpr
            && self
                .own_tokens(n)
                .first()
                .is_some_and(|t| ASSIGN_OPS.contains(t))
    }

    fn port_types(&self, body: NodeRef, scope: usize, env: &Env) -> Vec<(String, Ty)> {
        let mut out = Vec::new();
        let Some(pl) = self.child(body, PortList) else {
            return out;
        };
        let mut ty: Option<NodeRef> = None;
        for p in self.children(pl) {
            if self.kind(p) != Port {
                continue;
            }
            let own = self.own_tokens(p);
            let dir = own
                .iter()
                .any(|t| matches!(*t, "input" | "output" | "inout" | "ref"));
            match (dir, self.child(p, DataType)) {
                (_, Some(d)) => ty = Some(d),
                (true, None) => ty = None,
                (false, None) => {}
            }
            if let Some(d) = self.child(p, Declarator) {
                if let Some((n, _)) = self.name_of(d) {
                    out.push((n, self.decl_ty(ty, Some(d), scope, env)));
                }
            }
        }
        out
    }

    /// Formal directions and defaults of a subroutine's ports.
    fn formals(&self, decl: NodeRef) -> Vec<(Direction, Option<NodeRef>, String)> {
        let mut out = Vec::new();
        let Some(pl) = self.child(decl, PortList) else {
            return out;
        };
        let mut dir = Direction::In;
        for p in self.children(pl) {
            if self.kind(p) != Port {
                continue;
            }
            for t in self.own_tokens(p) {
                match t {
                    "input" => dir = Direction::In,
                    "output" => dir = Direction::Out,
                    "inout" => dir = Direction::InOut,
                    "ref" => dir = Direction::Ref,
                    _ => {}
                }
            }
            let default = self.child(p, Declarator).and_then(|d| {
                let has_eq = self.own_tokens(d).contains(&"=");
                has_eq
                    .then(|| {
                        self.children(d)
                            .into_iter()
                            .find(|c| !matches!(self.kind(*c), Name | Dim))
                    })
                    .flatten()
            });
            let name = self
                .child(p, Declarator)
                .and_then(|d| self.name_of(d))
                .map(|n| n.0)
                .unwrap_or_default();
            out.push((dir, default, name));
        }
        out
    }

    /// The type a declaration gives a name: its data type and the unpacked
    /// dimensions after the name.
    pub(crate) fn decl_ty(
        &self,
        dt: Option<NodeRef>,
        item: Option<NodeRef>,
        scope: usize,
        env: &Env,
    ) -> Ty {
        let mut ty = match dt {
            Some(d) if self.struct_of(d, scope, 0).is_some() => {
                let (sn, ss) = self.struct_of(d, scope, 0).unwrap();
                Ty::Struct(sn, ss)
            }
            Some(d) if self.array_typedef(d, scope).is_some() => {
                let (td, s, dim) = self.array_typedef(d, scope).unwrap();
                let elem = self.decl_ty(self.child(td, DataType), None, s, &Env::default());
                Ty::Array {
                    elem: Box::new(elem),
                    index: self.assoc_index(dim, s).map(Box::new),
                }
            }
            Some(d) => {
                let info = self.resolve_env(d, scope, env, 0);
                let t = match info.resolved {
                    Resolved::Class(c) => Ty::Class(c, info.args),
                    Resolved::Vif => Ty::Vif,
                    Resolved::Str => Ty::Str,
                    Resolved::Enum => Ty::Enum,
                    Resolved::Event => Ty::Event,
                    Resolved::Std(k) => Ty::Std(k),
                    Resolved::Other if self.undeclared_type(d) => Ty::Error,
                    Resolved::Other => Ty::Other,
                };
                if matches!(info.shape, TypeShape::Other | TypeShape::UnpackedStruct) {
                    t
                } else {
                    Ty::Array {
                        elem: Box::new(t),
                        index: None,
                    }
                }
            }
            None => Ty::Other,
        };
        if let Some(item) = item {
            let dims: Vec<NodeRef> = self
                .children(item)
                .into_iter()
                .filter(|c| self.kind(*c) == Dim)
                .collect();
            for d in dims.iter().rev() {
                let index = self.assoc_index(*d, scope);
                ty = Ty::Array {
                    elem: Box::new(ty),
                    index: index.map(Box::new),
                };
            }
        }
        ty
    }

    /// Whether a data type names a type declared nowhere.
    pub(crate) fn undeclared_type(&self, d: NodeRef) -> bool {
        let toks = self.tokens(d);
        let t = &self.trees[d.tree].tokens;
        let Some(&first) = toks.first() else {
            return false;
        };
        if t[first as usize].kind != sv_syntax::TokenKind::Identifier {
            return false;
        }
        let mut name = &*t[first as usize].text;
        if self.packages.contains_key(name) {
            match (toks.get(1), toks.get(2)) {
                (Some(&c), Some(&n)) if &*t[c as usize].text == "::" => name = &t[n as usize].text,
                _ => return false,
            }
        } else if toks.get(1).is_some_and(|&c| &*t[c as usize].text == "::") {
            return false;
        }
        !self.type_names.contains(name)
    }

    /// The struct a data type is (directly or through typedefs).
    fn struct_of(&self, d: NodeRef, scope: usize, depth: usize) -> Option<(NodeRef, usize)> {
        if depth > 8 {
            return None;
        }
        let toks = self.tokens(d);
        let first = self.text(d.tree, *toks.first()?);
        if matches!(first, "struct" | "union") {
            return Some((d, scope));
        }
        let (td, s) = self.typedef_named(d, scope)?;
        if self.children(td).iter().any(|c| self.kind(*c) == Dim) {
            return None;
        }
        self.struct_of(self.child(td, DataType)?, s, depth + 1)
    }

    /// The typedef a type name (`t` or `p::c::t`) stands for.
    fn typedef_named(&self, d: NodeRef, scope: usize) -> Option<(NodeRef, usize)> {
        let toks: Vec<&str> = self
            .tokens(d)
            .iter()
            .map(|&t| self.text(d.tree, t))
            .collect();
        let found = match toks.as_slice() {
            [one] => self.lookup(one, scope, 0),
            _ if toks.len() % 2 == 1 && toks.iter().skip(1).step_by(2).all(|t| *t == "::") => {
                let path: Vec<&str> = toks.iter().step_by(2).copied().collect();
                self.lookup_path(&path, scope, 0)
            }
            _ => None,
        };
        match found? {
            Found::Typedef(td, s) => Some((td, s)),
            _ => None,
        }
    }

    /// A field or member of a value of type `ty`.
    fn member_of(&self, ty: &Ty, name: &str) -> Option<Val> {
        match ty {
            Ty::Class(c, e) => self.class_member(*c, e, name),
            Ty::Struct(sn, ss) => {
                for d in self.children(*sn) {
                    if self.kind(d) != DataDecl {
                        continue;
                    }
                    for item in self.children(d) {
                        if self.kind(item) == Declarator
                            && self.name_of(item).is_some_and(|n| n.0 == name)
                        {
                            let t = self.decl_ty(
                                self.child(d, DataType),
                                Some(item),
                                *ss,
                                &Env::default(),
                            );
                            return Some(Val::Var(t, RootKind::Other));
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// A type name standing for a typedef with an unpacked dimension.
    fn array_typedef(&self, d: NodeRef, scope: usize) -> Option<(NodeRef, usize, NodeRef)> {
        let (td, s) = self.typedef_named(d, scope)?;
        let dim = self
            .children(td)
            .into_iter()
            .find(|c| self.kind(*c) == Dim)?;
        Some((td, s, dim))
    }

    /// The index type of an associative dimension `[T]`, if it is a type.
    fn assoc_index(&self, d: NodeRef, scope: usize) -> Option<Ty> {
        let toks = self.tokens(d);
        if toks.len() != 3 {
            return None;
        }
        let t = self.text(d.tree, toks[1]);
        if t == "string" {
            return Some(Ty::Str);
        }
        if let Some(k) = ["process", "semaphore", "mailbox"]
            .into_iter()
            .find(|k| *k == t)
        {
            if self.lookup(t, scope, 0).is_none() {
                return Some(Ty::Std(k));
            }
        }
        match self.lookup(t, scope, 0) {
            Some(Found::Class(c)) => Some(Ty::Class(c, Env::default())),
            Some(Found::Typedef(td, s)) => {
                let info = self.resolve_typedef(td, s, &Env::default(), 0);
                Some(match info.resolved {
                    Resolved::Class(c) => Ty::Class(c, info.args),
                    Resolved::Str => Ty::Str,
                    Resolved::Enum => Ty::Enum,
                    _ => Ty::Other,
                })
            }
            _ => None,
        }
    }

    /// The class (and its type arguments) of the value a name or member
    /// chain names: `x`, `this.x`, `a.b.c`, seen from `scope`. `local` is
    /// the declared type of a local variable the chain starts from.
    pub(crate) fn expr_class(&self, n: NodeRef, scope: usize, env: &Env, local: Option<NodeRef>, depth: usize) -> Option<(usize, Env)> {
        if depth > 16 {
            return None;
        }
        let as_class = |v: Option<Val>| match v {
            Some(Val::Var(Ty::Class(c, e), _)) => Some((c, e)),
            _ => None,
        };
        match self.kind(n) {
            ParenExpr => {
                let k = self.children(n).into_iter().find(|c| is_expr(self.kind(*c)))?;
                self.expr_class(k, scope, env, local, depth + 1)
            }
            NameExpr => {
                let toks = self.own_tokens(n);
                let [name] = toks.as_slice() else { return None };
                let own = self.class_at(scope);
                if *name == "this" {
                    return own.map(|c| (c, env.clone()));
                }
                if let Some(dt) = local {
                    let info = self.resolve_env(dt, scope, env, 0);
                    return match info.resolved {
                        Resolved::Class(c) => Some((c, info.args)),
                        _ => None,
                    };
                }
                as_class(self.class_member(own?, env, name))
            }
            MemberExpr => {
                let base = self.children(n).into_iter().find(|c| is_expr(self.kind(*c)))?;
                let name = *self.own_tokens(n).last()?;
                let (c, e) = self.expr_class(base, scope, env, local, depth + 1)?;
                as_class(self.class_member(c, &e, name))
            }
            _ => None,
        }
    }

    /// The class whose body `scope` is in.
    fn class_at(&self, scope: usize) -> Option<usize> {
        let mut cur = Some(scope);
        while let Some(s) = cur {
            if let ScopeKind::Class(c) = self.scopes[s].kind {
                return Some(c);
            }
            cur = self.scopes[s].parent;
        }
        None
    }

    /// Class `c`'s type arguments under `env`, each resolved to the type it
    /// stands for (None for a value parameter).
    pub(crate) fn resolved_type_args(&self, c: usize, env: &Env) -> Vec<Option<String>> {
        let full = self.complete_env(c, env);
        self.classes[c]
            .params
            .iter()
            .map(|(name, is_type, _)| {
                if !*is_type {
                    return None;
                }
                full.get(name).map(|b| self.ts(b.node, b.scope, &b.env, true, 1))
            })
            .collect()
    }

    /// `c` and its bases, each with its type arguments.
    fn chain_env(&self, c: usize, env: &Env) -> Vec<(usize, Env)> {
        let mut out = vec![(c, env.clone())];
        while out.len() <= self.classes.len() {
            let (x, e) = out.last().unwrap();
            let Some(next) = self.base_env(*x, e, 0) else {
                break;
            };
            out.push(next);
        }
        out
    }

    /// A member of class `c` (or a base) by name.
    fn class_member(&self, c: usize, env: &Env, name: &str) -> Option<Val> {
        for (x, e) in self.chain_env(c, env) {
            if let Some(m) = self.classes[x].members.get(name) {
                return Some(self.member_val(*m, Some(x), self.classes[x].scope, &e));
            }
        }
        if CLASS_BUILTINS.contains(&name) {
            let ret = if name == "get_randstate" {
                Ty::Str
            } else {
                Ty::Other
            };
            return Some(Val::Sub(Callee {
                system: false,
                scope: self.classes[c].name.clone(),
                declared_virtual: false,
                builtin: true,
                ret,
                formals: Vec::new(),
                receiver_is_arg: false,
                written: Written::None,
                ctx: None,
            }));
        }
        None
    }

    fn member_val(&self, m: Member, class: Option<usize>, scope: usize, env: &Env) -> Val {
        match m {
            Member::Var { decl, .. } if self.kind(decl) == CovergroupDecl => {
                Val::Var(Ty::Covergroup, RootKind::Property)
            }
            Member::Var { decl, item } => {
                let ty = self.decl_ty(self.child(decl, DataType), Some(item), scope, env);
                let kind = if class.is_some() {
                    RootKind::Property
                } else {
                    RootKind::Local
                };
                Val::Var(ty, kind)
            }
            Member::Sub { decl } => {
                let name = self.name_of(decl).map(|n| n.0).unwrap_or_default();
                let ret = self
                    .child(decl, ReturnType)
                    .and_then(|r| self.child(r, DataType))
                    .map_or(Ty::Other, |dt| self.decl_ty(Some(dt), None, scope, env));
                let own = self.own_tokens(decl);
                let mut declared_virtual = own.contains(&"virtual");
                if let Some(c) = class {
                    if !declared_virtual
                        && self.kind(decl) == MethodPrototype
                        && !own.contains(&"pure")
                        && name != "new"
                    {
                        declared_virtual = self.overrides_virtual(c, &name);
                    }
                }
                let owner = match class {
                    Some(c) => self.classes[c].name.clone(),
                    None => self.scopes[scope].name.clone(),
                };
                Val::Sub(Callee {
                    system: false,
                    scope: owner,
                    declared_virtual,
                    builtin: false,
                    ret,
                    // Defaults come from an out-of-block body when there is one.
                    formals: self.formals(
                        class
                            .filter(|_| self.kind(decl) == MethodPrototype)
                            .and_then(|c| {
                                self.bodies
                                    .get(&(self.classes[c].name.clone(), name.clone()))
                                    .copied()
                            })
                            .unwrap_or(decl),
                    ),
                    receiver_is_arg: false,
                    written: Written::None,
                    ctx: Some((class, env.clone(), scope)),
                })
            }
        }
    }

    /// A simple name seen from the body.
    fn lookup_val(&self, w: &Walk, name: &str) -> Option<Val> {
        for f in w.frames.iter().rev() {
            if let Some((ty, kind)) = f.get(name) {
                return Some(Val::Var(ty.clone(), *kind));
            }
        }
        let mut cur = Some(w.scope);
        while let Some(s) = cur {
            match self.scopes[s].kind {
                ScopeKind::Class(c) => {
                    let env = if c == w.class {
                        w.env.clone()
                    } else {
                        self.rep_env.get(&c).cloned().unwrap_or_default()
                    };
                    if let Some(v) = self.class_member(c, &env, name) {
                        return Some(v);
                    }
                }
                _ => {
                    if let Some(v) = self.scope_val(s, name) {
                        return Some(v);
                    }
                    for (pkg, item) in &self.scopes[s].imports {
                        if item.as_deref().is_some_and(|i| i != name) {
                            continue;
                        }
                        if let Some(&p) = self.packages.get(pkg) {
                            if let Some(v) = self.scope_val(p, name) {
                                return Some(v);
                            }
                        }
                    }
                }
            }
            cur = self.scopes[s].parent;
        }
        None
    }

    fn scope_val(&self, s: usize, name: &str) -> Option<Val> {
        let m = *self.scopes[s].values.get(name)?;
        Some(self.member_val(m, None, s, &Env::default()))
    }

    // ---- statements -------------------------------------------------------

    fn stmt(&mut self, w: &mut Walk, n: NodeRef) {
        let k = self.kind(n);
        match k {
            DataDecl => self.declare(w, n, RootKind::Local),
            SeqBlock | ParBlock => {
                w.frames.push(HashMap::new());
                // A function may hold `fork ... join_none`, timing and all.
                let saved = w.in_function;
                if k == ParBlock && self.own_tokens(n).contains(&"join_none") {
                    w.in_function = false;
                }
                for c in self.children(n) {
                    self.stmt(w, c);
                }
                w.in_function = saved;
                w.frames.pop();
            }
            IfStmt => {
                for c in self.children(n) {
                    if self.is_assign_op(c) {
                        self.poison(w, "assignment as an if condition");
                    }
                    self.stmt(w, c);
                }
            }
            TimedStmt => {
                if w.in_function {
                    self.poison(w, "timing control in a function");
                }
                let loc = self.first_loc(n);
                w.facts.timing.push(loc);
                for c in self.children(n) {
                    self.stmt(w, c);
                }
            }
            WaitStmt => {
                if self.own_tokens(n).get(1) != Some(&"fork") {
                    if w.in_function {
                        self.poison(w, "wait in a function");
                    }
                    let loc = self.first_loc(n);
                    w.facts.timing.push(loc);
                }
                for c in self.children(n) {
                    self.stmt(w, c);
                }
            }
            LoopStmt => {
                w.frames.push(HashMap::new());
                if self.own_tokens(n).first() == Some(&"foreach") {
                    self.foreach_vars(w, n);
                }
                for c in self.children(n) {
                    self.stmt(w, c);
                }
                w.frames.pop();
            }
            ExprStmt => {
                for c in self.children(n) {
                    if self.kind(c) == BinaryExpr && self.own_tokens(c).first() == Some(&"<=") {
                        self.assignment(w, c, true);
                    } else {
                        self.stmt(w, c);
                    }
                }
            }
            DoWhileStmt => {
                // slang visits the condition before the body.
                let kids = self.children(n);
                for c in &kids {
                    if is_expr(self.kind(*c)) {
                        self.expr(w, *c, None);
                    }
                }
                for c in kids {
                    if !is_expr(self.kind(c)) {
                        self.stmt(w, c);
                    }
                }
            }
            ReturnStmt => {
                let ret = w.ret.clone();
                for c in self.children(n) {
                    if is_expr(self.kind(c)) {
                        self.expr(w, c, Some(&ret));
                    }
                }
            }
            ConstraintBlock | WithClause | CovergroupDecl | ConcurrentAssert | TypedefDecl
            | ForwardTypedef => {}
            _ if is_expr(k) => self.expr(w, n, None),
            _ => {
                for c in self.children(n) {
                    self.stmt(w, c);
                }
            }
        }
    }

    fn declare(&mut self, w: &mut Walk, d: NodeRef, kind: RootKind) {
        let dt = self.child(d, DataType);
        for item in self.children(d) {
            if self.kind(item) != Declarator {
                continue;
            }
            if let Some((name, _)) = self.name_of(item) {
                let ty = self.decl_ty(dt, Some(item), w.scope, &w.env);
                w.frames.last_mut().unwrap().insert(name, (ty, kind));
            }
        }
    }

    /// `foreach (a.b[i, j])`: the loop variables, typed by the index type
    /// of an associative dimension.
    fn foreach_vars(&mut self, w: &mut Walk, n: NodeRef) {
        let toks: Vec<&str> = self.own_tokens(n).into_iter().collect();
        // foreach ( name . name [ vars ] ) stmt
        let Some(close) = toks.iter().rposition(|t| *t == "]") else {
            return;
        };
        let mut depth = 0;
        let mut open = close;
        for i in (0..close).rev() {
            match toks[i] {
                "]" => depth += 1,
                "[" if depth == 0 => {
                    open = i;
                    break;
                }
                "[" => depth -= 1,
                _ => {}
            }
        }
        let path: Vec<&str> = toks[2..open]
            .iter()
            .copied()
            .filter(|t| *t != ".")
            .collect();
        let mut ty = Ty::Unknown;
        for (k, seg) in path.iter().enumerate() {
            ty = if k == 0 {
                match self.lookup_val(w, seg) {
                    Some(Val::Var(t, _)) => t,
                    _ => Ty::Unknown,
                }
            } else {
                match self.member_of(&ty, seg) {
                    Some(Val::Var(t, _)) => t,
                    _ => Ty::Unknown,
                }
            };
        }
        let mut vars: Vec<Option<&str>> = vec![None];
        for t in &toks[open + 1..close] {
            if *t == "," {
                vars.push(None);
            } else {
                *vars.last_mut().unwrap() = Some(t);
            }
        }
        for v in vars {
            let (vt, next) = match ty {
                Ty::Array { elem, index } => (index.map_or(Ty::Other, |b| *b), *elem),
                _ => (Ty::Other, Ty::Unknown),
            };
            ty = next;
            if let Some(v) = v {
                w.frames
                    .last_mut()
                    .unwrap()
                    .insert(v.to_string(), (vt, RootKind::Other));
            }
        }
    }

    fn super_new_index(&mut self, body: NodeRef, facts: &mut BodyFacts) {
        let mut items = self.children(body);
        if items.len() == 1 && self.kind(items[0]) == SeqBlock {
            items = self.children(items[0]);
        }
        // Statements as slang lists them: one per declared variable.
        let mut list: Vec<NodeRef> = Vec::new();
        for s in items {
            match self.kind(s) {
                DataDecl => {
                    for d in self.children(s) {
                        if self.kind(d) == Declarator {
                            list.push(d);
                        }
                    }
                }
                EmptyStmt | EndLabel => {}
                k if k == ExprStmt
                    || k == IfStmt
                    || k == CaseStmt
                    || k == LoopStmt
                    || k == SeqBlock
                    || k == ParBlock
                    || k == TimedStmt
                    || k == WaitStmt
                    || k == ReturnStmt
                    || k == JumpStmt
                    || k == DisableStmt
                    || k == EventTrigger
                    || k == ProcAssign
                    || k == AssertStmt
                    || k == DoWhileStmt
                    || k == RandSequenceStmt =>
                {
                    list.push(s)
                }
                _ => {}
            }
        }
        if list.len() < 2 {
            return;
        }
        for (i, s) in list.iter().enumerate() {
            if self.kind(*s) == ExprStmt && self.is_super_new_stmt(*s) {
                let loc = self.first_loc(*s);
                facts.super_new = Some((i as u32, loc));
                return;
            }
        }
    }

    fn is_super_new_stmt(&self, s: NodeRef) -> bool {
        let Some(e) = self.children(s).into_iter().next() else {
            return false;
        };
        let callee = if self.kind(e) == CallExpr {
            match self.children(e).into_iter().next() {
                Some(c) => c,
                None => return false,
            }
        } else {
            e
        };
        self.is_super_new(callee)
    }

    fn is_super_new(&self, callee: NodeRef) -> bool {
        if self.kind(callee) != MemberExpr {
            return false;
        }
        let toks = self.tokens(callee);
        toks.len() == 3
            && self.text(callee.tree, toks[0]) == "super"
            && self.text(callee.tree, toks[2]) == "new"
    }

    // ---- expressions ------------------------------------------------------

    fn expr(&mut self, w: &mut Walk, n: NodeRef, target: Option<&Ty>) {
        match self.kind(n) {
            BinaryExpr => {
                let op = self.own_tokens(n).first().copied().unwrap_or("");
                if ASSIGN_OPS.contains(&op) {
                    self.assignment(w, n, false);
                } else {
                    self.sub_exprs(w, n);
                }
            }
            CallExpr => self.call(w, n, false),
            NameExpr | ScopedExpr | MemberExpr => {
                let toks = self.tokens(n);
                let first = toks.first().map_or("", |&t| self.text(n.tree, t));
                if self.kind(n) == NameExpr && matches!(first, "$root" | "$unit") {
                    // A hierarchical prefix, not a call.
                    return;
                }
                if self.kind(n) == NameExpr && first.starts_with('$') && first.len() > 1 {
                    // `$time`: a system call without parentheses.
                    let callee = self.system_callee(first);
                    self.record_call(w, n, n, first.to_string(), &callee, None, false);
                    return;
                }
                if self.is_super_new(n) {
                    self.super_new_call(w, n, None);
                    return;
                }
                let name = self.text(n.tree, *toks.last().unwrap()).to_string();
                if first == "super"
                    && self.classes[w.class].extends.is_some()
                    && self.base_env(w.class, &w.env, 0).is_none()
                {
                    self.poison(w, "super of an undeclared base");
                }
                match self.resolve_name(w, n) {
                    Some(Val::Var(Ty::Error, _)) if self.kind(n) == NameExpr => {
                        self.poison(w, "a name of an undeclared type");
                    }
                    Some(Val::Sub(callee)) => {
                        self.record_call(w, n, n, name, &callee, None, false);
                    }
                    None if self.kind(n) == MemberExpr => {
                        // `ev.triggered`, `q.size`, `p.status`: built-in
                        // methods called without parentheses.
                        let left = self.children(n)[0];
                        let callee = match self.ty_of(w, left) {
                            Ty::Std(k) => Some(self.std_method(k, &name)),
                            t @ (Ty::Array { .. } | Ty::Str | Ty::Enum | Ty::Event)
                                if is_value_method(&name) =>
                            {
                                Some(self.value_method(&t, &name))
                            }
                            _ => None,
                        };
                        if let Some(c) = callee {
                            self.record_call(w, n, n, name, &c, None, false);
                        }
                    }
                    _ => {}
                }
                if self.kind(n) == MemberExpr {
                    if let Some(l) = self.children(n).into_iter().next() {
                        self.expr(w, l, None);
                    }
                } else if self.kind(n) == ScopedExpr {
                    // Type arguments hold no calls slang reports.
                }
            }
            NewExpr => self.new_call(w, n, target),
            CastExpr => {
                // The type before the tick is not an expression. Inside
                // `T'(...)` (not `void'`) slang's call syntax is the parens.
                let void = self
                    .children(n)
                    .first()
                    .is_some_and(|t| self.own_tokens(*t) == ["void"]);
                for c in self.children(n).into_iter().skip(1) {
                    if self.kind(c) == CallExpr {
                        self.call(w, c, !void);
                    } else if is_expr(self.kind(c)) {
                        self.expr(w, c, None);
                    }
                }
            }
            CondExpr => {
                let kids = self.children(n);
                for (i, c) in kids.iter().enumerate() {
                    if is_expr(self.kind(*c)) {
                        self.expr(w, *c, if i > 0 { target } else { None });
                    }
                }
            }
            ParenExpr => {
                for c in self.children(n) {
                    if self.is_assign_op(c) {
                        // `(a = b)`: slang's syntax for the assignment is
                        // the parens, so it has no right side.
                        let i = w.facts.assignments.len();
                        self.assignment(w, c, false);
                        if let Some(a) = w.facts.assignments.get_mut(i) {
                            a.rhs.clear();
                        }
                    } else if self.kind(c) == CallExpr {
                        self.call(w, c, true);
                    } else if is_expr(self.kind(c)) {
                        self.expr(w, c, target);
                    }
                }
            }
            TypeExpr | Literal => {}
            _ => self.sub_exprs(w, n),
        }
    }

    fn sub_exprs(&mut self, w: &mut Walk, n: NodeRef) {
        for c in self.children(n) {
            let k = self.kind(c);
            if is_expr(k) {
                self.expr(w, c, None);
            } else if !matches!(k, ConstraintBlock | WithClause | DataType) {
                self.sub_exprs(w, c);
            }
        }
    }

    fn assignment(&mut self, w: &mut Walk, n: NodeRef, non_blocking: bool) {
        let kids: Vec<NodeRef> = self
            .children(n)
            .into_iter()
            .filter(|c| is_expr(self.kind(*c)))
            .collect();
        let (Some(&lhs), rhs) = (kids.first(), kids.get(1).copied()) else {
            return;
        };
        if rhs.is_some_and(|r| self.is_assign_op(r)) {
            self.poison(w, "chained assignment");
        }
        let (root_kind, root_name) = self.root_of(w, lhs);
        let loc = self.first_loc(n);
        let a = Assignment {
            lhs: self.src_text(lhs),
            rhs: rhs.map_or(String::new(), |r| self.src_text(r)),
            is_non_blocking: non_blocking,
            root_kind,
            root_name,
            loc,
        };
        w.facts.assignments.push(a);
        self.expr(w, lhs, None);
        if let Some(r) = rhs {
            let target = self.ty_of(w, lhs);
            self.expr(w, r, Some(&target));
        }
    }

    /// An argument written by the call: an assignment with no right side.
    fn written_arg(&mut self, w: &mut Walk, e: NodeRef) {
        let (root_kind, root_name) = self.root_of(w, e);
        let loc = self.first_loc(e);
        let a = Assignment {
            lhs: self.src_text(e),
            rhs: String::new(),
            is_non_blocking: false,
            root_kind,
            root_name,
            loc,
        };
        w.facts.assignments.push(a);
    }

    fn root_of(&self, w: &Walk, e: NodeRef) -> (RootKind, String) {
        let mut cur = e;
        loop {
            match self.kind(cur) {
                MemberExpr | SelectExpr => match self.children(cur).into_iter().next() {
                    Some(l) => cur = l,
                    None => return (RootKind::Other, String::new()),
                },
                NameExpr | ScopedExpr => {
                    let toks = self.tokens(cur);
                    let Some(&last) = toks.last() else {
                        return (RootKind::Other, String::new());
                    };
                    let name = self.text(cur.tree, last).to_string();
                    if name == "this" {
                        return (RootKind::Local, name);
                    }
                    let kind = match self.resolve_name(w, cur) {
                        Some(Val::Var(_, k)) => k,
                        _ => RootKind::Other,
                    };
                    return (kind, name);
                }
                _ => return (RootKind::Other, String::new()),
            }
        }
    }

    /// A name expression: `x`, `a::b`, `x.y`.
    fn resolve_name(&self, w: &Walk, n: NodeRef) -> Option<Val> {
        match self.kind(n) {
            NameExpr => {
                let toks = self.tokens(n);
                if toks.len() != 1 {
                    return None;
                }
                let t = self.text(n.tree, toks[0]);
                match t {
                    "this" => Some(Val::Var(Ty::Class(w.class, w.env.clone()), RootKind::Local)),
                    "super" => self
                        .base_env(w.class, &w.env, 0)
                        .map(|(b, e)| Val::Var(Ty::Class(b, e), RootKind::Other)),
                    _ => self.lookup_val(w, t),
                }
            }
            MemberExpr => {
                let kids = self.children(n);
                let left = *kids.first()?;
                let toks = self.own_tokens(n);
                let name = *toks.last()?;
                self.member_of(&self.ty_of(w, left), name)
            }
            ScopedExpr => {
                let kids = self.children(n);
                let left = *kids.first()?;
                let name = *self.own_tokens(n).last()?;
                let ltoks = self.tokens(left);
                let lfirst = ltoks.first().map_or("", |&t| self.text(left.tree, t));
                if ltoks.len() == 1 {
                    if let Some(&p) = self.packages.get(lfirst) {
                        return self.scope_val(p, name);
                    }
                }
                let info = self.resolve_env(left, w.scope, &w.env, 0);
                match info.resolved {
                    Resolved::Class(c) => {
                        let env = if c == w.class && info.args.is_empty() {
                            w.env.clone()
                        } else if info.args.is_empty() {
                            self.rep_env.get(&c).cloned().unwrap_or_default()
                        } else {
                            info.args
                        };
                        self.class_member(c, &env, name)
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn ty_of(&self, w: &Walk, n: NodeRef) -> Ty {
        match self.kind(n) {
            NameExpr | MemberExpr | ScopedExpr => match self.resolve_name(w, n) {
                Some(Val::Var(t, _)) => t,
                Some(Val::Sub(c)) => c.ret,
                None => {
                    // `obj.method` on a value that is not a class.
                    Ty::Unknown
                }
            },
            SelectExpr => {
                let Some(l) = self.children(n).into_iter().next() else {
                    return Ty::Unknown;
                };
                let range = self
                    .own_tokens(n)
                    .iter()
                    .any(|t| matches!(*t, ":" | "+:" | "-:"));
                match self.ty_of(w, l) {
                    a @ Ty::Array { .. } if range => a,
                    Ty::Array { elem, .. } => *elem,
                    _ => Ty::Other,
                }
            }
            CallExpr => {
                let Some(callee) = self.children(n).into_iter().next() else {
                    return Ty::Unknown;
                };
                if self.kind(callee) == CallExpr {
                    return self.ty_of(w, callee);
                }
                self.resolve_callee(w, callee)
                    .map_or(Ty::Unknown, |(c, _)| c.ret)
            }
            ParenExpr => self
                .children(n)
                .into_iter()
                .find(|c| is_expr(self.kind(*c)))
                .map_or(Ty::Unknown, |c| self.ty_of(w, c)),
            CastExpr => {
                let Some(t) = self.children(n).into_iter().next() else {
                    return Ty::Unknown;
                };
                let info = self.resolve_env(t, w.scope, &w.env, 0);
                match info.resolved {
                    Resolved::Class(c) => Ty::Class(c, info.args),
                    Resolved::Str => Ty::Str,
                    Resolved::Enum => Ty::Enum,
                    _ => Ty::Other,
                }
            }
            CondExpr => {
                let kids: Vec<NodeRef> = self
                    .children(n)
                    .into_iter()
                    .filter(|c| is_expr(self.kind(*c)))
                    .collect();
                match kids.get(1).map(|&k| self.ty_of(w, k)) {
                    Some(t @ Ty::Class(..)) => t,
                    _ => kids.get(2).map_or(Ty::Unknown, |&k| self.ty_of(w, k)),
                }
            }
            Literal => Ty::Other,
            _ => Ty::Other,
        }
    }

    fn system_callee(&self, name: &str) -> Callee {
        let written = match name {
            "$cast" | "$sformat" | "$swrite" | "$swriteb" | "$swriteh" | "$swriteo" | "$fgets"
            | "$fread" => Written::At(0),
            "$value$plusargs" => Written::At(1),
            "$sscanf" | "$fscanf" => Written::From(2),
            _ => Written::None,
        };
        let ret = match name {
            "$sformatf" | "$psprintf" | "$typename" => Ty::Str,
            _ => Ty::Other,
        };
        Callee {
            system: true,
            scope: String::new(),
            declared_virtual: false,
            builtin: false,
            ret,
            formals: Vec::new(),
            receiver_is_arg: false,
            written,
            ctx: None,
        }
    }

    /// A built-in method on a value that is not a class handle.
    fn value_method(&self, recv: &Ty, name: &str) -> Callee {
        let ret = match recv {
            Ty::Array { elem, .. } => match name {
                "pop_front" | "pop_back" => (**elem).clone(),
                "find" | "find_first" | "find_last" | "unique" | "min" | "max" => recv.clone(),
                _ => Ty::Other,
            },
            Ty::Str => match name {
                "substr" | "toupper" | "tolower" => Ty::Str,
                _ => Ty::Other,
            },
            Ty::Enum => match name {
                "name" => Ty::Str,
                "first" | "last" | "next" | "prev" => Ty::Enum,
                _ => Ty::Other,
            },
            _ => Ty::Other,
        };
        Callee {
            system: true,
            scope: String::new(),
            declared_virtual: false,
            builtin: false,
            ret,
            formals: Vec::new(),
            receiver_is_arg: true,
            written: Written::None,
            ctx: None,
        }
    }

    fn std_method(&self, class: &'static str, name: &str) -> Callee {
        let ret = match (class, name) {
            ("process", "self") => Ty::Std("process"),
            ("process", "get_randstate") => Ty::Str,
            ("process", "status") => Ty::Enum,
            _ => Ty::Other,
        };
        Callee {
            system: false,
            scope: class.to_string(),
            declared_virtual: false,
            builtin: true,
            ret,
            formals: Vec::new(),
            receiver_is_arg: false,
            written: Written::None,
            ctx: None,
        }
    }

    /// What the callee expression of a call calls, and the receiver
    /// expression (for `x.f()`).
    fn resolve_callee(&self, w: &Walk, callee: NodeRef) -> Option<(Callee, Option<NodeRef>)> {
        let own = self.own_tokens(callee);
        match self.kind(callee) {
            NameExpr => {
                let name = *own.first()?;
                if name.starts_with('$') {
                    return Some((self.system_callee(name), None));
                }
                if name == w.own_name {
                    // A recursive call, not the return variable.
                    let mut ww = Walk {
                        frames: Vec::new(),
                        ..w.shallow()
                    };
                    ww.frames.clear();
                    return match self.lookup_val(&ww, name) {
                        Some(Val::Sub(c)) => Some((c, None)),
                        _ => None,
                    };
                }
                if name == "randomize" {
                    return Some((self.system_callee(name), None));
                }
                match self.lookup_val(w, name) {
                    Some(Val::Sub(c)) => Some((c, None)),
                    _ => None,
                }
            }
            MemberExpr => {
                let left = self.children(callee).into_iter().next()?;
                let name = *own.last()?;
                if name == "randomize" {
                    let mut c = self.system_callee(name);
                    c.receiver_is_arg = true;
                    return Some((c, Some(left)));
                }
                let recv = self.ty_of(w, left);
                let c = match &recv {
                    Ty::Class(c, e) => match self.class_member(*c, e, name) {
                        Some(Val::Sub(c)) => c,
                        _ => return None,
                    },
                    Ty::Std(k) => self.std_method(k, name),
                    Ty::Vif | Ty::Unknown | Ty::Error | Ty::Struct(..) => return None,
                    // `cg.sample(...)`, `cg.get_coverage()`: built in.
                    Ty::Covergroup => Callee {
                        system: false,
                        scope: String::new(),
                        declared_virtual: false,
                        builtin: true,
                        ret: Ty::Other,
                        formals: Vec::new(),
                        receiver_is_arg: false,
                        written: Written::None,
                        ctx: None,
                    },
                    _ => self.value_method(&recv, name),
                };
                Some((c, Some(left)))
            }
            ScopedExpr => {
                let left = self.children(callee).into_iter().next()?;
                let name = *own.last()?;
                let ltoks = self.tokens(left);
                let lfirst = ltoks.first().map_or("", |&t| self.text(left.tree, t));
                if ltoks.len() == 1 {
                    match lfirst {
                        "std" if name == "randomize" => {
                            let mut c = self.system_callee(name);
                            c.written = Written::From(0);
                            return Some((c, None));
                        }
                        "process" | "semaphore" | "mailbox" if !self.is_user_type(w, lfirst) => {
                            let k = match lfirst {
                                "process" => "process",
                                "semaphore" => "semaphore",
                                _ => "mailbox",
                            };
                            return Some((self.std_method(k, name), None));
                        }
                        _ => {}
                    }
                }
                match self.resolve_name(w, callee) {
                    Some(Val::Sub(c)) => Some((c, None)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn is_user_type(&self, w: &Walk, name: &str) -> bool {
        self.lookup(name, w.scope, 0).is_some()
    }

    fn call(&mut self, w: &mut Walk, n: NodeRef, parens: bool) {
        let kids = self.children(n);
        let Some(&callee) = kids.first() else {
            return;
        };
        let with = self.child(n, WithClause);
        if self.kind(callee) == CallExpr {
            // `f(...) with ...`: the call is the inner one, and slang's
            // syntax for it is the whole expression.
            self.call(w, callee, true);
            if let Some(wc) = with {
                self.with_clause(w, callee, wc);
            }
            return;
        }
        let parens = parens || with.is_some();
        let args = self.child(n, ArgList);
        if self.is_super_new(callee) {
            self.super_new_call(w, n, args);
            return;
        }
        let name = self
            .own_tokens(callee)
            .last()
            .copied()
            .unwrap_or("")
            .to_string();
        let resolved = self.resolve_callee(w, callee);
        let (c, recv) = match resolved {
            Some(x) => x,
            None => {
                // Unknown: slang has no call for a name it cannot bind, but
                // a value method on a receiver of unknown type is kept.
                let recv = (self.kind(callee) == MemberExpr)
                    .then(|| self.children(callee).into_iter().next())
                    .flatten();
                // `c.constraint_mode(0)` on a constraint is a value method too.
                let vm = recv.is_some() && (is_value_method(&name) || name == "constraint_mode");
                if !vm {
                    if self.kind(callee) == NameExpr && !self.sub_names.contains(&name) {
                        self.poison(w, "a call of an undeclared subroutine");
                    }
                    if let Some(r) = recv {
                        self.expr(w, r, None);
                    }
                    if let Some(al) = args {
                        self.args_plain(w, al);
                    }
                    if let Some(wc) = with {
                        self.with_clause(w, n, wc);
                    }
                    return;
                }
                (
                    Callee {
                        system: vm,
                        scope: String::new(),
                        declared_virtual: false,
                        builtin: false,
                        ret: Ty::Unknown,
                        formals: Vec::new(),
                        receiver_is_arg: vm,
                        written: Written::None,
                        ctx: None,
                    },
                    recv,
                )
            }
        };
        self.record_call(w, n, callee, name, &c, args, parens);
        if let Some(r) = recv {
            self.expr(w, r, None);
        }
        self.walk_args(w, args, &c);
        if let Some(wc) = with {
            self.with_clause(w, n, wc);
        }
    }

    /// `q.find(x) with (x.f())`: the expression, with the iterator bound to
    /// an element of the array (`item` unless named).
    fn with_clause(&mut self, w: &mut Walk, call: NodeRef, wc: NodeRef) {
        let Some(e) = self
            .children(wc)
            .into_iter()
            .find(|c| is_expr(self.kind(*c)))
        else {
            return;
        };
        let callee = self.children(call)[0];
        let iter = self
            .child(call, ArgList)
            .and_then(|al| self.children(al).into_iter().find(|a| self.kind(*a) == Arg))
            .and_then(|a| self.children(a).into_iter().next())
            .map(|n| self.own_tokens(n).concat())
            .unwrap_or_else(|| "item".to_string());
        let elem = match self.kind(callee) {
            MemberExpr => match self.ty_of(w, self.children(callee)[0]) {
                Ty::Array { elem, .. } => *elem,
                _ => Ty::Other,
            },
            _ => Ty::Other,
        };
        let mut frame = HashMap::new();
        frame.insert(iter, (elem, RootKind::Other));
        w.frames.push(frame);
        self.expr(w, e, None);
        w.frames.pop();
    }

    /// The actual arguments in order, and the defaults of missing ones
    /// (evaluated where the subroutine is declared).
    fn walk_args(&mut self, w: &mut Walk, args: Option<NodeRef>, c: &Callee) {
        let actuals = self.actuals(args, c);
        let n = actuals.len().max(c.formals.len());
        for k in 0..n {
            match actuals.get(k).copied().flatten() {
                Some(e) if self.kind(e) == DataType => {}
                Some(e) => {
                    let written = match c.written {
                        Written::At(i) => i == k,
                        Written::From(i) => k >= i,
                        Written::None => c
                            .formals
                            .get(k)
                            .is_some_and(|f| matches!(f.0, Direction::Out | Direction::InOut)),
                    };
                    if written {
                        self.written_arg(w, e);
                    }
                    self.expr(w, e, None);
                }
                None => {
                    let (Some(d), Some((class, env, scope))) =
                        (c.formals.get(k).and_then(|f| f.1), c.ctx.as_ref())
                    else {
                        continue;
                    };
                    let mut dw = Walk {
                        own_name: String::new(),
                        in_function: false,
                        class: class.unwrap_or(w.class),
                        env: env.clone(),
                        scope: *scope,
                        frames: Vec::new(),
                        ret: Ty::Other,
                        facts: BodyFacts::default(),
                    };
                    self.expr(&mut dw, d, None);
                    w.facts.calls.append(&mut dw.facts.calls);
                    w.facts.assignments.append(&mut dw.facts.assignments);
                }
            }
        }
    }

    /// Actual arguments in formal order (named ones placed by name); an
    /// argument may be a type (`$bits(T)`).
    fn actuals(&self, args: Option<NodeRef>, c: &Callee) -> Vec<Option<NodeRef>> {
        let mut out: Vec<Option<NodeRef>> = Vec::new();
        let Some(al) = args else {
            return out;
        };
        for a in self.children(al) {
            if self.kind(a) != Arg {
                continue;
            }
            let e = self
                .children(a)
                .into_iter()
                .find(|e| is_expr(self.kind(*e)) || self.kind(*e) == DataType);
            let own = self.own_tokens(a);
            if own.first() == Some(&".") {
                let name = own.get(1).copied().unwrap_or("");
                match c.formals.iter().position(|f| f.2 == name) {
                    Some(i) => {
                        if out.len() <= i {
                            out.resize(i + 1, None);
                        }
                        out[i] = e;
                    }
                    None => out.push(e),
                }
            } else {
                out.push(e);
            }
        }
        out
    }

    fn super_new_call(&mut self, w: &mut Walk, n: NodeRef, args: Option<NodeRef>) {
        w.facts.calls_super_new = true;
        let mut callee = None;
        match self.base_env(w.class, &w.env, 0) {
            Some((b, e)) => {
                if let Some(m @ Member::Sub { .. }) = self.classes[b].members.get("new").copied() {
                    if let Val::Sub(c) = self.member_val(m, Some(b), self.classes[b].scope, &e) {
                        if self.ctor_bad(w, &c, args) {
                            return;
                        }
                        self.record_ctor(w, n, &c, args);
                        callee = Some(c);
                    }
                }
            }
            None if self.classes[w.class].extends.is_some() => {
                self.poison(w, "super.new of an undeclared base");
            }
            None => {}
        }
        match callee {
            Some(c) => self.walk_args(w, args, &c),
            None => {
                if let Some(al) = args {
                    self.args_plain(w, al);
                }
            }
        }
    }

    fn args_plain(&mut self, w: &mut Walk, al: NodeRef) {
        for a in self.children(al) {
            if self.kind(a) != Arg {
                continue;
            }
            for e in self.children(a) {
                if is_expr(self.kind(e)) {
                    self.expr(w, e, None);
                }
            }
        }
    }

    fn new_call(&mut self, w: &mut Walk, n: NodeRef, target: Option<&Ty>) {
        let args = self.child(n, ArgList);
        if self.own_tokens(n).contains(&"[")
            || (args.is_none() && self.children(n).into_iter().any(|c| is_expr(self.kind(c))))
        {
            // `new[n]` / `new[n](old)`: an array; `new obj`: a shallow
            // copy. Neither calls a constructor.
            self.sub_exprs(w, n);
            return;
        }
        let callee = match target {
            Some(Ty::Error) => {
                self.poison(w, "new of an undeclared type");
                None
            }
            Some(Ty::Class(c, e)) => {
                let (c, e) = (*c, e.clone());
                if self.is_virtual_class(c) {
                    self.poison(w, "new of a virtual class");
                }
                match self.classes[c].members.get("new").copied() {
                    Some(m @ Member::Sub { .. }) => {
                        match self.member_val(m, Some(c), self.classes[c].scope, &e) {
                            Val::Sub(callee) => Some(callee),
                            _ => None,
                        }
                    }
                    _ => None,
                }
            }
            Some(Ty::Std(k)) if *k != "process" => Some(self.std_method(k, "new")),
            _ => None,
        };
        match callee {
            Some(c) if self.ctor_bad(w, &c, args) => {}
            Some(c) => {
                self.record_ctor(w, n, &c, args);
                self.walk_args(w, args, &c);
            }
            None => {
                if let Some(al) = args {
                    self.args_plain(w, al);
                }
            }
        }
    }

    fn record_ctor(&mut self, w: &mut Walk, n: NodeRef, c: &Callee, args: Option<NodeRef>) {
        let loc = self.first_loc(n);
        let (null, arg_loc) = self.first_arg(args, c, None);
        w.facts.calls.push(Call {
            name: "new".into(),
            is_system: false,
            callee_scope: c.scope.clone(),
            callee_declared_virtual: false,
            callee_builtin: c.builtin,
            first_arg_is_null: null,
            first_arg_loc: arg_loc,
            receiver: String::new(),
            via_super: false,
            loc,
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn record_call(
        &mut self,
        w: &mut Walk,
        n: NodeRef,
        callee: NodeRef,
        name: String,
        c: &Callee,
        args: Option<NodeRef>,
        parens: bool,
    ) {
        let loc = self.first_loc(n);
        // In parentheses, slang's syntax for the call is the parenthesized
        // expression: no receiver, and it does not start with `super`.
        let receiver = if parens {
            String::new()
        } else {
            self.receiver_text(callee)
        };
        let recv_node = (self.kind(callee) == MemberExpr)
            .then(|| self.children(callee).into_iter().next())
            .flatten();
        let (null, arg_loc) =
            self.first_arg(args, c, if c.receiver_is_arg { recv_node } else { None });
        let via_super = !parens
            && self
                .tokens(n)
                .first()
                .is_some_and(|&t| self.text(n.tree, t) == "super");
        w.facts.calls.push(Call {
            name,
            is_system: c.system,
            callee_scope: c.scope.clone(),
            callee_declared_virtual: c.declared_virtual,
            callee_builtin: c.builtin,
            first_arg_is_null: null,
            first_arg_loc: arg_loc,
            receiver,
            via_super,
            loc,
        });
    }

    /// Whether the first argument is `null`, and where it is: the
    /// receiver of a value method, the first actual or the default.
    fn first_arg(
        &mut self,
        args: Option<NodeRef>,
        c: &Callee,
        recv: Option<NodeRef>,
    ) -> (bool, Loc) {
        let first = if let Some(r) = recv {
            Some(r)
        } else {
            let actual = self.actuals(args, c).first().copied().flatten();
            actual.or_else(|| c.formals.first().and_then(|f| f.1))
        };
        match first {
            Some(mut e) => {
                // slang places a parenthesized expression at its inside.
                while self.kind(e) == ParenExpr {
                    match self
                        .children(e)
                        .into_iter()
                        .find(|c| is_expr(self.kind(*c)))
                    {
                        Some(x) => e = x,
                        None => break,
                    }
                }
                let null = self.kind(e) == NameExpr && self.own_tokens(e) == ["null"];
                (null, self.first_loc(e))
            }
            None => (false, Loc::default()),
        }
    }

    /// `x.f()`'s receiver as written when the callee is a name (`a.b[i].f`),
    /// empty otherwise.
    fn receiver_text(&self, callee: NodeRef) -> String {
        if self.kind(callee) != MemberExpr {
            return String::new();
        }
        let Some(left) = self.children(callee).into_iter().next() else {
            return String::new();
        };
        if !self.is_name(left) {
            return String::new();
        }
        let t = self.tree(left);
        let toks = self.tokens(left);
        let mut s = String::new();
        for &k in &toks {
            let tok = &t.tokens[k as usize];
            if !tok.from_macro {
                s.push_str(&printed_trivia(t.trivia(k)));
            }
            s.push_str(&tok.text);
        }
        // slang prints the name with its leading comments, less whitespace.
        s.trim_start_matches([' ', '\t', '\r', '\n']).to_string()
    }

    /// A name in slang's sense: identifiers, `this`, `super`, selects and
    /// member accesses on them.
    fn is_name(&self, n: NodeRef) -> bool {
        match self.kind(n) {
            NameExpr | ScopedExpr => true,
            MemberExpr | SelectExpr => self
                .children(n)
                .into_iter()
                .next()
                .is_some_and(|l| self.is_name(l)),
            _ => false,
        }
    }

    /// The source text of a node, when it was written in one file and not
    /// made by a macro (as slang's model takes it from the source buffer).
    fn src_text(&self, n: NodeRef) -> String {
        let t = self.tree(n);
        let toks = self.tokens(n);
        let (Some(&a), Some(&b)) = (toks.first(), toks.last()) else {
            return String::new();
        };
        let (ta, tb) = (&t.tokens[a as usize], &t.tokens[b as usize]);
        if ta.from_macro || tb.from_macro || ta.origin.file != tb.origin.file {
            return String::new();
        }
        let src = &t.sources[ta.origin.file as usize];
        let (lo, hi) = (
            ta.origin.offset as usize,
            tb.origin.offset as usize + tb.text.len(),
        );
        if lo > hi || hi > src.len() {
            return String::new();
        }
        src[lo..hi].to_string()
    }

    fn first_loc(&mut self, n: NodeRef) -> Loc {
        match self.tokens(n).first() {
            Some(&t) => self.loc(n.tree, t),
            None => Loc::default(),
        }
    }
}

/// Built-in methods of arrays, strings, enums and events.
fn is_value_method(name: &str) -> bool {
    matches!(
        name,
        "size"
            | "num"
            | "exists"
            | "delete"
            | "insert"
            | "push_back"
            | "push_front"
            | "pop_back"
            | "pop_front"
            | "first"
            | "last"
            | "next"
            | "prev"
            | "len"
            | "substr"
            | "toupper"
            | "tolower"
            | "compare"
            | "icompare"
            | "atoi"
            | "atohex"
            | "atooct"
            | "atobin"
            | "atoreal"
            | "itoa"
            | "hextoa"
            | "octtoa"
            | "bintoa"
            | "realtoa"
            | "putc"
            | "getc"
            | "name"
            | "find"
            | "find_index"
            | "find_first"
            | "find_first_index"
            | "find_last"
            | "find_last_index"
            | "min"
            | "max"
            | "unique"
            | "unique_index"
            | "reverse"
            | "sort"
            | "rsort"
            | "shuffle"
            | "sum"
            | "product"
            | "and"
            | "or"
            | "xor"
            | "triggered"
    )
}

/// Leading trivia as slang prints a name: comments and whitespace, without
/// directives, disabled code or anything before a macro use, and with runs of
/// line breaks collapsed.
fn printed_trivia(raw: &str) -> String {
    let b = raw.as_bytes();
    let mut out = String::new();
    // Text between conditional directives: kept only if it is comments.
    let mut region_start: Option<usize> = None;
    let mut i = 0;
    let ident_end = |mut j: usize| {
        while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_' || b[j] == b'$') {
            j += 1;
        }
        j
    };
    let skip_ws = |mut j: usize| {
        while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
            j += 1;
        }
        j
    };
    let mut code_in_region = false;
    while i < b.len() {
        let c = b[i];
        if c == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            let e = raw[i..].find('\n').map_or(b.len(), |n| i + n);
            out.push_str(&raw[i..e]);
            i = e;
        } else if c == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            let e = raw[i + 2..].find("*/").map_or(b.len(), |n| i + 2 + n + 2);
            out.push_str(&raw[i..e]);
            i = e;
        } else if c.is_ascii_whitespace() {
            out.push(c as char);
            i += 1;
        } else if c == b'`' {
            let e = ident_end(i + 1);
            let name = &raw[i + 1..e];
            let mut j = e;
            match name {
                "ifdef" | "ifndef" | "elsif" | "else" | "endif" => {
                    if matches!(name, "ifdef" | "ifndef" | "elsif") {
                        j = ident_end(skip_ws(j));
                    }
                    if let Some(st) = region_start.take() {
                        if code_in_region {
                            out.truncate(st);
                        }
                    }
                    code_in_region = false;
                    if name != "endif" {
                        region_start = Some(out.len());
                    }
                }
                "define" | "undef" | "timescale" | "default_nettype" | "line" | "pragma"
                | "include" | "begin_keywords" | "unconnected_drive" => {
                    // To the end of the line, with continuations.
                    while j < b.len() && b[j] != b'\n' {
                        if b[j] == b'\\' && j + 1 < b.len() && b[j + 1] == b'\n' {
                            j += 1;
                        }
                        j += 1;
                    }
                }
                "resetall"
                | "undefineall"
                | "celldefine"
                | "endcelldefine"
                | "nounconnected_drive"
                | "end_keywords" => {}
                _ if region_start.is_some() => code_in_region = true,
                _ => {
                    // A macro use: its expansion owns what came before.
                    let k = skip_ws(j);
                    if k < b.len() && b[k] == b'(' {
                        let mut depth = 0;
                        let mut q = k;
                        let mut in_str = false;
                        while q < b.len() {
                            match b[q] {
                                b'\\' if in_str => q += 1,
                                b'"' => in_str = !in_str,
                                b'(' if !in_str => depth += 1,
                                b')' if !in_str => {
                                    depth -= 1;
                                    if depth == 0 {
                                        q += 1;
                                        break;
                                    }
                                }
                                _ => {}
                            }
                            q += 1;
                        }
                        j = q;
                    }
                    out.clear();
                }
            }
            i = j.max(i + 1);
        } else {
            // Disabled code.
            code_in_region = true;
            i += 1;
        }
    }
    if let Some(st) = region_start {
        if code_in_region {
            out.truncate(st);
        }
    }
    let mut collapsed = String::with_capacity(out.len());
    for ch in out.chars() {
        if ch == '\n' && collapsed.ends_with('\n') {
            continue;
        }
        collapsed.push(ch);
    }
    collapsed
}
