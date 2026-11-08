//! Declared types for expression typing (builder phase N6): the type a
//! declaration, data type or untyped parameter has, as slang prints it
//! (`Type::toString`) and as its integral shape (width, signedness, four
//! states), seen from where it is written. Rules type expressions on top
//! of this; see `dv-lint`'s expression typer and tools/exprdump.cpp.

use super::types::INT_TYPES;
use super::*;

/// A type as an expression typer needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeInfo {
    /// `Type::toString()`.
    pub text: String,
    /// An integral type (packed): its width, signedness and four states.
    pub integral: Option<(u32, bool, bool)>,
    /// The data type it was declared with, to look into (struct members).
    pub src: Option<(usize, sv_syntax::NodeId)>,
    /// An enum, struct or union (also through typedefs): binary operators
    /// do not keep it as their type.
    pub aggregate: bool,
}

impl TypeInfo {
    /// A type with only its printed form: not integral, not an aggregate, no
    /// declaration to look into.
    pub fn other(text: impl Into<String>) -> TypeInfo {
        TypeInfo { text: text.into(), integral: None, src: None, aggregate: false }
    }
}

/// The builder's scopes and constant evaluation, read-only.
pub struct TypeOracle<'t> {
    b: Builder<'t>,
    regions: Vec<crate::ScopeMap>,
    /// Answers already given (constant functions make some slow).
    decls: std::cell::RefCell<HashMap<(usize, Option<sv_syntax::NodeId>, Option<sv_syntax::NodeId>), TypeInfo>>,
    values: std::cell::RefCell<HashMap<(usize, sv_syntax::NodeId), TypeInfo>>,
    consts: std::cell::RefCell<HashMap<(usize, sv_syntax::NodeId), Option<(i128, u32, bool)>>>,
    /// Enum members and package members by (scope, package, name).
    members: std::cell::RefCell<HashMap<(usize, String, String), Option<TypeInfo>>>,
    /// The specialization of a generic class to type its body in (see
    /// [`Self::set_spec`]); otherwise its representative.
    spec: std::cell::RefCell<Option<(usize, Env)>>,
    /// An enum typedef's type, by (tree, typedef, scope), where no
    /// specialization applies.
    enum_types: std::cell::RefCell<HashMap<(usize, sv_syntax::NodeId, usize), Option<TypeInfo>>>,
}

impl<'t> TypeOracle<'t> {
    /// Builds the scopes and constant evaluation for `trees` once; every query
    /// after that is read-only (answers are cached).
    pub fn new(trees: &'t [Tree]) -> Self {
        let mut b = Builder::new(trees);
        b.designs = true;
        b.block_consts = true;
        b.collect_names();
        b.collect();
        let regions = (0..trees.len()).map(|t| b.scope_map(t)).collect();
        TypeOracle { b, regions, decls: Default::default(), values: Default::default(), consts: Default::default(), members: Default::default(), spec: Default::default(), enum_types: Default::default() }
    }

    fn scope_of(&self, tree: usize, n: sv_syntax::NodeId) -> usize {
        match self.b.trees[tree].token_bounds(n).map(|b| b.0) {
            Some(tok) => Builder::scope_at(&self.regions[tree], tok, self.b.unit),
            None => self.b.unit,
        }
    }

    /// The generic class around a node and how many specializations slang
    /// elaborates of it (`None` outside a generic class).
    pub fn specs_at(&self, tree: usize, n: sv_syntax::NodeId) -> Option<(usize, usize)> {
        let g = self.b.generic_owner(self.scope_of(tree, n))?;
        Some((g, self.b.spec_sets.get(&g).map_or(0, |m| m.len())))
    }

    /// Types generic class `g`'s body in its `i`th specialization (as
    /// [`Self::specs_at`] counts them), or as before with `None`. Answers
    /// already given are dropped.
    pub fn set_spec(&self, spec: Option<(usize, usize)>) {
        let env = spec.and_then(|(g, i)| Some((g, self.b.spec_sets.get(&g)?.values().nth(i)?.1.clone())));
        *self.spec.borrow_mut() = env;
        self.decls.borrow_mut().clear();
        self.values.borrow_mut().clear();
        self.consts.borrow_mut().clear();
        self.members.borrow_mut().clear();
    }

    /// The scope's environment: a class's type parameters at their defaults
    /// (or as the specialization being typed binds them).
    fn env_of(&self, scope: usize) -> Env {
        let mut cur = Some(scope);
        while let Some(s) = cur {
            if let ScopeKind::Class(c) = self.b.scopes[s].kind {
                if let Some(env) = self.spec.borrow().as_ref().filter(|x| x.0 == c).map(|x| x.1.clone()) {
                    return env;
                }
                return self.b.rep_env.get(&c).cloned().unwrap_or_default();
            }
            cur = self.b.scopes[s].parent;
        }
        Env::default()
    }

    /// A declaration's type: its data type (`None` for an implicit one)
    /// and the declarator (for unpacked dimensions).
    pub fn decl_type(
        &self,
        tree: usize,
        dt: Option<sv_syntax::NodeId>,
        item: Option<sv_syntax::NodeId>,
    ) -> TypeInfo {
        if let Some(t) = self.decls.borrow().get(&(tree, dt, item)) {
            return t.clone();
        }
        let t = self.decl_type_uncached(tree, dt, item);
        self.decls.borrow_mut().insert((tree, dt, item), t.clone());
        t
    }

    fn decl_type_uncached(
        &self,
        tree: usize,
        dt: Option<sv_syntax::NodeId>,
        item: Option<sv_syntax::NodeId>,
    ) -> TypeInfo {
        let at = item.or(dt).unwrap_or(self.b.trees[tree].root);
        let scope = self.scope_of(tree, at);
        let env = self.env_of(scope);
        let r = |n| NodeRef { tree, node: n };
        let text = self.b.type_string(dt.map(r), item.map(r), scope, &env, false);
        let unpacked = item.is_some_and(|i| self.b.trees[tree].children(i).any(|c| self.b.trees[tree].kind(c) == Dim));
        let integral = match (dt, unpacked) {
            (_, true) => None,
            (Some(d), _) => self.shape(r(d), scope, &env),
            (None, _) => Some((1, false, true)),
        };
        let aggregate = dt.is_some_and(|d| self.aggregate(r(d), 0));
        TypeInfo { text, integral, src: dt.filter(|_| !unpacked).map(|d| (tree, d)), aggregate }
    }

    /// A data type written in an expression (`T'(x)`, a type parameter).
    pub fn data_type(&self, tree: usize, dt: sv_syntax::NodeId) -> TypeInfo {
        self.decl_type(tree, Some(dt), None)
    }

    /// The type of an untyped parameter's value.
    pub fn value_type(&self, tree: usize, v: sv_syntax::NodeId) -> TypeInfo {
        if let Some(t) = self.values.borrow().get(&(tree, v)) {
            return t.clone();
        }
        let t = self.value_type_uncached(tree, v);
        self.values.borrow_mut().insert((tree, v), t.clone());
        t
    }

    fn value_type_uncached(&self, tree: usize, v: sv_syntax::NodeId) -> TypeInfo {
        let scope = self.scope_of(tree, v);
        let env = self.env_of(scope);
        let r = NodeRef { tree, node: v };
        let text = self.b.value_type(r, scope, &env);
        let integral = self.b.eval(r, scope, &env, 0).map(|c| (c.w.max(1), c.s, true));
        TypeInfo { text, integral, src: None, aggregate: false }
    }

    /// The type of an enum member named at `at`: the enum, printed whole.
    pub fn enum_member(&self, tree: usize, at: sv_syntax::NodeId, name: &str) -> Option<TypeInfo> {
        let scope = self.scope_of(tree, at);
        let key = (scope, String::new(), name.to_string());
        if let Some(t) = self.members.borrow().get(&key) {
            return t.clone();
        }
        let t = self.enum_member_in(scope, name);
        self.members.borrow_mut().insert(key, t.clone());
        t
    }

    fn enum_member_in(&self, scope: usize, name: &str) -> Option<TypeInfo> {
        let env = self.env_of(scope);
        let (t, s) = self.b.enum_typedef(name, scope)?;
        if !env.is_empty() {
            return self.enum_type(t, s, &env);
        }
        let key = (t.tree, t.node, s);
        if let Some(v) = self.enum_types.borrow().get(&key) {
            return v.clone();
        }
        let v = self.enum_type(t, s, &env);
        self.enum_types.borrow_mut().insert(key, v.clone());
        v
    }

    fn enum_type(&self, t: crate::NodeRef, s: usize, env: &Env) -> Option<TypeInfo> {
        let text = self.b.typedef_target(t, s, env);
        let dt = self.b.child(t, DataType)?;
        Some(TypeInfo { text, integral: self.shape(dt, s, env), src: Some((dt.tree, dt.node)), aggregate: true })
    }

    /// The type of `pkg::name`: a package variable, parameter or enum member.
    pub fn package_member(&self, pkg: &str, name: &str) -> Option<TypeInfo> {
        let key = (usize::MAX, pkg.to_string(), name.to_string());
        if let Some(t) = self.members.borrow().get(&key) {
            return t.clone();
        }
        let t = self.package_member_uncached(pkg, name);
        self.members.borrow_mut().insert(key, t.clone());
        t
    }

    fn package_member_uncached(&self, pkg: &str, name: &str) -> Option<TypeInfo> {
        let b = &self.b;
        let &p = b.packages.get(pkg)?;
        let sc = &b.scopes[p];
        if let Some(Member::Var { decl, item }) = sc.values.get(name) {
            return Some(self.decl_type(item.tree, b.child(*decl, DataType).map(|d| d.node), Some(item.node)));
        }
        if let Some(&(dt, v)) = sc.consts.get(name) {
            let typed = dt.is_some_and(|d| !b.toks_of(d).iter().all(|t| matches!(*t, "signed" | "unsigned")) || b.child(d, Dim).is_some());
            return match (typed, dt, v) {
                (true, Some(d), _) => Some(self.decl_type(d.tree, Some(d.node), None)),
                (_, _, Some(v)) => Some(self.value_type(v.tree, v.node)),
                _ => None,
            };
        }
        let (t, s) = b.enum_typedef(name, p)?;
        let env = Env::default();
        let text = b.typedef_target(t, s, &env);
        let dt = b.child(t, DataType)?;
        Some(TypeInfo { text, integral: self.shape(dt, s, &env), src: Some((dt.tree, dt.node)), aggregate: true })
    }

    /// The type with its typedefs resolved, for matching (IEEE 1800 6.22.1):
    /// an enum, struct or union by its declaration's name, `reg` as `logic`,
    /// `realtime` as `real`.
    pub fn canonical(&self, t: &TypeInfo) -> String {
        let norm = |s: &str| match s {
            "reg" => "logic".to_string(),
            "realtime" => "real".to_string(),
            _ => s.replace("reg[", "logic[").replace("reg signed", "logic signed"),
        };
        if t.aggregate {
            return t.text.rsplit_once('}').map_or(t.text.clone(), |(_, n)| n.to_string());
        }
        let Some((tree, node)) = t.src else { return norm(&t.text) };
        let b = &self.b;
        let mut dt = NodeRef { tree, node };
        for _ in 0..16 {
            let scope = self.scope_of(dt.tree, dt.node);
            match b.lookup_type(dt, scope) {
                Some(Found::Typedef(td, _)) if b.children(td).into_iter().all(|c| b.kind(c) != Dim) => match b.child(td, DataType) {
                    Some(i) => dt = i,
                    None => break,
                },
                Some(Found::Param(..)) => {
                    let name = b.toks_of(dt).concat();
                    match self.env_of(scope).get(&name) {
                        Some(bd) => dt = bd.node,
                        None => break,
                    }
                }
                _ => break,
            }
        }
        let r = self.decl_type(dt.tree, Some(dt.node), None);
        if r.aggregate {
            return t.text.rsplit_once('}').map_or(t.text.clone(), |(_, n)| n.to_string());
        }
        norm(&r.text)
    }

    /// The class declaration (tree, node) a declared type names, through
    /// typedefs and specializations.
    pub fn class_decl(&self, tree: usize, dt: sv_syntax::NodeId) -> Option<(usize, sv_syntax::NodeId)> {
        let b = &self.b;
        let scope = self.scope_of(tree, dt);
        let env = self.env_of(scope);
        match b.resolve_env(NodeRef { tree, node: dt }, scope, &env, 0).resolved {
            crate::Resolved::Class(c) => Some((b.classes[c].node.tree, b.classes[c].node.node)),
            _ => None,
        }
    }

    /// For a name standing for a class type (`uvm_config_db#(int)`, or a
    /// typedef of one): the class's name and the type its parameter
    /// `param` is bound to.
    pub fn class_arg(&self, tree: usize, n: sv_syntax::NodeId, param: &str) -> Option<(String, Option<TypeInfo>)> {
        let b = &self.b;
        let scope = self.scope_of(tree, n);
        let env = self.env_of(scope);
        let ti = b.resolve_env(NodeRef { tree, node: n }, scope, &env, 0);
        let crate::Resolved::Class(c) = ti.resolved else { return None };
        let arg = ti.args.get(param).map(|bd| self.decl_type(bd.node.tree, Some(bd.node.node), None));
        Some((b.classes[c].name.clone(), arg))
    }

    /// The result of a method of class declaration `class` whose return
    /// type `ret` is one of its type parameters, called on a value declared
    /// with type `recv`: the type the receiver's specialization binds it to,
    /// printed as slang prints the parameter.
    pub fn method_result(
        &self,
        tree: usize,
        recv: sv_syntax::NodeId,
        class: (usize, sv_syntax::NodeId),
        ret: sv_syntax::NodeId,
    ) -> Option<TypeInfo> {
        let b = &self.b;
        let r = NodeRef { tree, node: recv };
        let scope = self.scope_of(tree, recv);
        let env = self.env_of(scope);
        let ti = b.resolve_env(r, scope, &env, 0);
        let crate::Resolved::Class(c) = ti.resolved else { return None };
        if (b.classes[c].node.tree, b.classes[c].node.node) != class {
            return None;
        }
        let name = b.toks_of(NodeRef { tree: class.0, node: ret }).concat();
        let bd = ti.args.get(&name)?;
        let shown = self.data_type(class.0, ret);
        let actual = self.decl_type(bd.node.tree, Some(bd.node.node), None);
        Some(TypeInfo { text: shown.text, ..actual })
    }

    /// The type of member `name` of a struct or union typed value.
    pub fn member(&self, base: &TypeInfo, name: &str) -> Option<TypeInfo> {
        let b = &self.b;
        // A virtual interface's signal.
        let rest = base.text.strip_prefix("virtual ").unwrap_or(&base.text);
        let rest = rest.strip_prefix("interface ").unwrap_or(rest);
        let iface = rest.split(['#', '.', ' ']).next().unwrap_or("");
        if let Some(&decl) = b.interfaces.get(iface).filter(|_| base.integral.is_none()) {
            let mut todo = vec![decl];
            while let Some(n) = todo.pop() {
                for k in b.children(n) {
                    match b.kind(k) {
                        DataDecl | Port => {
                            for d in b.children(k) {
                                if b.kind(d) == Declarator && b.name_of(d).is_some_and(|x| x.0 == name) {
                                    return Some(self.decl_type(d.tree, b.child(k, DataType).map(|x| x.node), Some(d.node)));
                                }
                            }
                        }
                        Header | PortList | Body | GenerateRegion => todo.push(k),
                        _ => {}
                    }
                }
            }
            return None;
        }
        let (tree, node) = base.src?;
        let mut dt = NodeRef { tree, node };
        for _ in 0..16 {
            if matches!(b.own_tokens(dt).first().copied(), Some("struct" | "union")) {
                for m in b.children(dt) {
                    if b.kind(m) != DataDecl {
                        continue;
                    }
                    for d in b.children(m) {
                        if b.kind(d) == Declarator && b.name_of(d).is_some_and(|n| n.0 == name) {
                            return Some(self.decl_type(d.tree, b.child(m, DataType).map(|x| x.node), Some(d.node)));
                        }
                    }
                }
                return None;
            }
            let scope = self.scope_of(dt.tree, dt.node);
            match b.lookup_type(dt, scope) {
                Some(Found::Typedef(t, _)) if b.children(t).into_iter().all(|c| b.kind(c) != Dim) => dt = b.child(t, DataType)?,
                _ => return None,
            }
        }
        None
    }

    /// A constant expression's value, width and signedness.
    pub fn eval(&self, tree: usize, e: sv_syntax::NodeId) -> Option<(i128, u32, bool)> {
        if let Some(t) = self.consts.borrow().get(&(tree, e)) {
            return *t;
        }
        let t = self.eval_uncached(tree, e);
        self.consts.borrow_mut().insert((tree, e), t);
        t
    }

    fn eval_uncached(&self, tree: usize, e: sv_syntax::NodeId) -> Option<(i128, u32, bool)> {
        let scope = self.scope_of(tree, e);
        let env = self.env_of(scope);
        let c = self.b.eval(NodeRef { tree, node: e }, scope, &env, 0)?;
        c.known().then(|| (c.i(), c.w, c.s))
    }

    fn aggregate(&self, dt: NodeRef, depth: usize) -> bool {
        let b = &self.b;
        if depth > 16 {
            return false;
        }
        if matches!(b.own_tokens(dt).first().copied(), Some("enum" | "struct" | "union")) {
            return true;
        }
        let scope = self.scope_of(dt.tree, dt.node);
        match b.lookup_type(dt, scope) {
            Some(Found::Typedef(t, _)) => b.child(t, DataType).is_some_and(|i| self.aggregate(i, depth + 1)),
            _ => false,
        }
    }

    /// Width, signedness and four states of an integral data type.
    fn shape(&self, dt: NodeRef, scope: usize, env: &Env) -> Option<(u32, bool, bool)> {
        let (w, s) = self.b.int_shape(dt, scope, env, 0)?;
        Some((w, s, self.four_state(dt, scope, env, 0)))
    }

    /// The type holds x and z: logic, reg, integer, time, an implicit
    /// type, or an aggregate or enum built on one.
    fn four_state(&self, dt: NodeRef, scope: usize, env: &Env, depth: usize) -> bool {
        if depth > 24 {
            return false;
        }
        let b = &self.b;
        let own = b.own_tokens(dt);
        if let Some(k) = own.iter().find(|t| INT_TYPES.contains(t)) {
            return matches!(*k, "logic" | "reg" | "integer" | "time");
        }
        match own.first().copied() {
            Some("enum") => return b.child(dt, DataType).is_some_and(|i| self.four_state(i, scope, env, depth + 1)),
            Some("struct" | "union") => {
                return b.children(dt).into_iter().filter(|m| b.kind(*m) == DataDecl).any(|m| {
                    b.child(m, DataType).is_some_and(|i| self.four_state(i, scope, env, depth + 1))
                })
            }
            _ => {}
        }
        let toks = b.toks_of(dt);
        // Implicit (`[7:0] x`, `signed x`): no type name before the dimensions.
        let named = toks
            .iter()
            .take_while(|t| **t != "[")
            .any(|t| t.starts_with(|c: char| c.is_alphabetic() || c == '_' || c == '\\') && !matches!(*t, "signed" | "unsigned"));
        if !named {
            return true;
        }
        if let [name] = toks.as_slice() {
            if let Some(bd) = env.get(*name) {
                return self.four_state(bd.node, bd.scope, &bd.env, depth + 1);
            }
        }
        match b.lookup_type(dt, scope) {
            Some(Found::Typedef(t, s)) => b.child(t, DataType).is_some_and(|i| self.four_state(i, s, env, depth + 1)),
            Some(Found::Param(Some(d), s)) => self.four_state(d, s, env, depth + 1),
            _ => false,
        }
    }
}
