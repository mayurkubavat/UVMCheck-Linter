//! Expression types over the class index (what slang's `Expression::type`
//! gives the C++ rules) and class metrics (`metrics::classMetrics`).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use svlang::syntax::{NodeId, SyntaxKind};

use crate::support::statements::*;

use crate::support::class_index::*;

use crate::support::routine_metrics::routine_metrics;

use crate::syntax::Cst;

use SyntaxKind::*;

/// A resolved type: its name (through typedefs), the class it is, and
/// whether it is an unpacked array of that.
#[derive(Clone, Debug)]
pub(crate) struct Ty {
    pub name: String,
    pub class: Option<usize>,
    pub array: bool,
}

/// Where a name was declared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    Local,
    /// A property of this class (index) or one of its ancestors.
    Prop(usize),
    Global,
}

/// Names visible in one routine of one class.
pub(crate) struct Env<'a, 'c> {
    pub c: &'a Cst<'c>,
    pub classes: &'a Classes,
    pub class: Option<usize>,
    pub(crate) locals: HashMap<String, (Option<NodeId>, bool)>,
    pub(crate) globals: &'a HashMap<String, (Option<NodeId>, bool)>,
}

/// Variables declared outside classes and routines, by name.
pub(crate) fn globals(c: &Cst) -> HashMap<String, (Option<NodeId>, bool)> {
    let mut out = HashMap::new();
    for d in c.nodes(DataDecl) {
        let Some(p) = c.parent(d) else { continue };
        if !matches!(c.kind(p), SourceFile | PackageDecl | ModuleDecl | InterfaceDecl | ProgramDecl) {
            continue;
        }
        let ty = c.tree.child(d, DataType);
        for decl in c.tree.children(d).filter(|&x| c.kind(x) == Declarator) {
            if let Some(nm) = c.tree.child(decl, Name) {
                let array = c.tree.child(decl, Dim).is_some();
                out.entry(c.tree.bare_text(nm)).or_insert((ty, array));
            }
        }
    }
    out
}

impl<'a, 'c> Env<'a, 'c> {
    pub fn new(
        c: &'a Cst<'c>,
        classes: &'a Classes,
        class: Option<usize>,
        routine: Option<NodeId>,
        globals: &'a HashMap<String, (Option<NodeId>, bool)>,
    ) -> Self {
        let mut locals = HashMap::new();
        if let Some(r) = routine {
            for p in ports(c, r) {
                locals.insert(p.name.clone(), (p.ty, false));
            }
            for d in subtree(c, r).into_iter().filter(|&x| c.kind(x) == DataDecl) {
                let mut ty = c.tree.child(d, DataType);
                // `for (int i = 0, j = 1; ...)`: the second declaration
                // takes the first one's type.
                if ty.is_none() {
                    if let Some(prev) = prev_sibling_decl(c, d) {
                        ty = c.tree.child(prev, DataType);
                    }
                }
                for decl in c.tree.children(d).filter(|&x| c.kind(x) == Declarator) {
                    if let Some(nm) = c.tree.child(decl, Name) {
                        let array = c.tree.child(decl, Dim).is_some();
                        locals.insert(c.tree.bare_text(nm), (ty, array));
                    }
                }
            }
        }
        Env { c, classes, class, locals, globals }
    }

    pub fn ty_of(&self, ty: Option<NodeId>, array: bool) -> Option<Ty> {
        let ty = ty?;
        let mut name = type_name(self.c, ty)?;
        // A typedef of the class around (or a base) hides one of the same
        // name elsewhere: every UVM class has its own `this_type`.
        if let Some(t) = self.class_typedef(&name) {
            name = t;
        }
        let class = self.classes.find(&name);
        Some(Ty { name: self.classes.resolve_name(&name), class, array })
    }

    /// The target of typedef `name` declared in the class around or one of
    /// its bases, nearest first.
    fn class_typedef(&self, name: &str) -> Option<String> {
        let c = self.c;
        let mut cur = self.class?;
        for _ in 0..64 {
            let node = self.classes.list[cur].node;
            let hit = c.tree.children(node).filter(|&x| c.kind(x) == TypedefDecl).find_map(|t| {
                let nm = c.tree.child(t, Name)?;
                (c.tree.bare_text(nm) == name).then(|| c.tree.child(t, DataType).and_then(|d| type_name(c, d)))?
            });
            if hit.is_some() {
                return hit;
            }
            cur = self.classes.base(cur)?;
        }
        None
    }

    /// The type of a name and where it was declared.
    pub fn lookup(&self, name: &str) -> Option<(Ty, Origin)> {
        if let Some(&(ty, array)) = self.locals.get(name) {
            return self.ty_of(ty, array).map(|t| (t, Origin::Local));
        }
        if let Some(i) = self.class {
            if let Some((k, p)) = self.prop(i, name) {
                let array = p.1;
                return self.ty_of(p.0, array).map(|t| (t, Origin::Prop(k)));
            }
        }
        let &(ty, array) = self.globals.get(name)?;
        self.ty_of(ty, array).map(|t| (t, Origin::Global))
    }

    /// Property `name` of class `i` or an ancestor: declaring class, type.
    pub fn prop(&self, i: usize, name: &str) -> Option<(usize, (Option<NodeId>, bool))> {
        let k = property_owner(self.classes, i, name)?;
        let p = self.classes.list[k].props.iter().find(|p| p.name == name)?;
        Some((k, (p.ty, p.array)))
    }

    /// The declared type node of a name or member expression.
    pub fn decl_type(&self, e: NodeId) -> Option<NodeId> {
        let c = self.c;
        let e = strip_parens(c, e);
        match c.kind(e) {
            NameExpr => {
                let name = ident(c, e)?;
                if let Some(&(ty, _)) = self.locals.get(name) {
                    return ty;
                }
                if let Some(i) = self.class {
                    if let Some((_, (ty, _))) = self.prop(i, name) {
                        return ty;
                    }
                }
                self.globals.get(name).and_then(|&(ty, _)| ty)
            }
            MemberExpr => {
                let recv = c.tree.children(e).next()?;
                let k = self.expr_type(recv).filter(|t| !t.array)?.class?;
                let (_, b) = c.tree.token_bounds(e)?;
                self.prop(k, c.text_of(b))?.1 .0
            }
            _ => None,
        }
    }

    /// slang's `Expression::type`, for the expressions rules care about.
    pub fn expr_type(&self, e: NodeId) -> Option<Ty> {
        let c = self.c;
        let e = strip_parens(c, e);
        match c.kind(e) {
            NameExpr => {
                let t = c.toks(e).next().map(|t| c.text_of(t))?;
                if t == "this" {
                    let i = self.class?;
                    return Some(Ty { name: self.classes.list[i].name.clone(), class: Some(i), array: false });
                }
                if t == "super" {
                    let b = self.classes.base(self.class?)?;
                    return Some(Ty { name: self.classes.list[b].name.clone(), class: Some(b), array: false });
                }
                self.lookup(ident(c, e)?).map(|(t, _)| t)
            }
            MemberExpr => {
                let recv = c.tree.children(e).next()?;
                let rt = self.expr_type(recv)?;
                if rt.array {
                    return None;
                }
                let k = rt.class?;
                let (_, b) = c.tree.token_bounds(e)?;
                let (_, (ty, array)) = self.prop(k, c.text_of(b))?;
                self.ty_of(ty, array)
            }
            SelectExpr => {
                let base = c.tree.children(e).next()?;
                let t = self.expr_type(base)?;
                Some(Ty { array: false, ..t })
            }
            CallExpr => {
                let (k, m) = self.callee(e)?;
                let ret = c.tree.child(self.classes.list[k].methods[m].decl, ReturnType)?;
                self.ty_of(c.tree.child(ret, DataType), false)
            }
            CastExpr => {
                let t = c.tree.children(e).next()?;
                let name = c.tree.bare_text(t);
                let class = self.classes.find(&name);
                class.map(|k| Ty { name: self.classes.list[k].name.clone(), class: Some(k), array: false })
            }
            _ => None,
        }
    }

    /// The method a call resolves to: class index and method index.
    pub fn callee(&self, call: NodeId) -> Option<(usize, usize)> {
        let c = self.c;
        let callee = c.tree.children(call).next()?;
        match c.kind(callee) {
            NameExpr => {
                let name = ident(c, callee)?;
                self.method(self.class?, name)
            }
            MemberExpr => {
                let recv = c.tree.children(callee).next()?;
                let rt = self.expr_type(recv)?;
                if rt.array {
                    return None;
                }
                let (_, b) = c.tree.token_bounds(callee)?;
                self.method(rt.class?, c.text_of(b))
            }
            CallExpr => self.callee(callee),
            ScopedExpr => {
                let toks: Vec<u32> = c.tree.token_range(callee);
                let n = toks.len();
                if n < 3 || c.text_of(toks[n - 2]) != "::" {
                    return None;
                }
                let k = self.classes.find(c.text_of(toks[n - 3]))?;
                self.method(k, c.text_of(toks[n - 1]))
            }
            _ => None,
        }
    }

    /// Method `name` of class `i` or its nearest ancestor declaring it.
    pub fn method(&self, i: usize, name: &str) -> Option<(usize, usize)> {
        let mut cur = Some(i);
        let mut guard = 0;
        while let Some(k) = cur {
            guard += 1;
            if guard > 64 {
                break;
            }
            if let Some(m) = self.classes.list[k].methods.iter().position(|m| m.name == name) {
                return Some((k, m));
            }
            cur = self.classes.base(k);
        }
        None
    }

    /// `X::type_id::create(...)` returns an X.
    pub fn factory_create(&self, call: NodeId) -> Option<usize> {
        let c = self.c;
        let callee = c.tree.children(call).next()?;
        if c.kind(callee) != ScopedExpr {
            return None;
        }
        let toks: Vec<&str> = c.tree.token_range(callee).iter().map(|&t| c.text_of(t)).collect();
        if toks.len() == 5 && toks[1] == "::" && toks[2] == "type_id" && toks[3] == "::" && toks[4] == "create" {
            return self.classes.find(toks[0]);
        }
        None
    }
}

pub(crate) fn prev_sibling_decl(c: &Cst, d: NodeId) -> Option<NodeId> {
    let p = c.parent(d)?;
    let kids: Vec<NodeId> = c.tree.children(p).collect();
    let i = kids.iter().position(|&x| x == d)?;
    kids[..i].iter().rev().copied().find(|&x| c.kind(x) == DataDecl && c.tree.child(x, DataType).is_some())
}

/// `a.m` as the callee of a call is a method reference, not a member access.
pub(crate) fn is_method_ref(c: &Cst, env: &Env, e: NodeId) -> bool {
    let Some(p) = c.parent(e) else { return false };
    if c.kind(p) == CallExpr && c.tree.children(p).next() == Some(e) {
        return true;
    }
    // `a.m` with no parentheses calling a method.
    let Some(recv) = c.tree.children(e).next() else {
        return false;
    };
    let Some(t) = env.expr_type(recv) else { return false };
    let Some(k) = t.class else { return false };
    let Some((_, b)) = c.tree.token_bounds(e) else {
        return false;
    };
    env.prop(k, c.text_of(b)).is_none() && env.method(k, c.text_of(b)).is_some()
}

pub(crate) struct ClassMetrics {
    pub cls: usize,
    pub wmc: i64,
    pub atfd: usize,
    pub tcc: f64,
    pub lcom_groups: Vec<Vec<String>>,
    pub cbo: usize,
    pub coupled_to: Vec<String>,
    pub coupled: Vec<usize>,
    pub dit: usize,
    pub ancestry: Vec<String>,
    pub fields: usize,
    pub methods: usize,
}

pub(crate) struct Usage<'e, 'a, 'c> {
    pub(crate) env: &'e Env<'a, 'c>,
    pub(crate) own_fields: BTreeSet<String>,
    pub(crate) own_calls: BTreeSet<String>,
    pub(crate) foreign: BTreeSet<String>,
    pub(crate) types: BTreeSet<String>,
    pub(crate) type_class: BTreeMap<String, usize>,
}

impl Usage<'_, '_, '_> {
    pub(crate) fn is_own(&self, k: usize) -> bool {
        Some(k) == self.env.class
    }

    pub(crate) fn note(&mut self, t: Option<Ty>) {
        let Some(t) = t else { return };
        let Some(k) = t.class else { return };
        if self.env.classes.list[k].library || self.is_own(k) {
            return;
        }
        let name = self.env.classes.list[k].name.clone();
        self.types.insert(name.clone());
        self.type_class.entry(name).or_insert(k);
    }

    pub(crate) fn walk(&mut self, root: NodeId) {
        let c = self.env.c;
        let classes = self.env.classes;
        let me = self.env.class.unwrap();
        for x in subtree(c, root) {
            match c.kind(x) {
                NameExpr => {
                    // Callees are calls, not values.
                    if c.parent(x).is_some_and(|p| c.kind(p) == CallExpr && c.tree.children(p).next() == Some(x)) {
                        continue;
                    }
                    let Some(name) = ident(c, x) else { continue };
                    let Some((t, origin)) = self.env.lookup(name) else {
                        continue;
                    };
                    if origin == Origin::Prop(me) {
                        self.own_fields.insert(name.to_string());
                    }
                    self.note(Some(t));
                }
                MemberExpr => {
                    if is_method_ref(c, self.env, x) {
                        continue;
                    }
                    let Some(recv) = c.tree.children(x).next() else {
                        continue;
                    };
                    let Some((_, b)) = c.tree.token_bounds(x) else { continue };
                    let member = c.text_of(b);
                    if let Some(rt) = self.env.expr_type(recv).filter(|t| !t.array) {
                        if let Some(k) = rt.class {
                            if let Some((owner, _)) = self.env.prop(k, member) {
                                if self.is_own(owner) {
                                    self.own_fields.insert(member.to_string());
                                } else if !classes.list[owner].library && !classes.derives_from(me, owner) {
                                    self.foreign.insert(format!("{}::{member}", classes.list[owner].name));
                                }
                            }
                        }
                    }
                    self.note(self.env.expr_type(x));
                }
                CallExpr => {
                    if let Some((k, m)) = self.env.callee(x) {
                        if self.is_own(k) {
                            self.own_calls.insert(classes.list[k].methods[m].name.clone());
                        }
                    }
                    let t = match self.env.factory_create(x) {
                        Some(k) => Some(Ty { name: classes.list[k].name.clone(), class: Some(k), array: false }),
                        None => self.env.expr_type(x),
                    };
                    self.note(t);
                }
                DataDecl => {
                    let ty = c.tree.child(x, DataType);
                    if ty.is_some() {
                        let t = self.env.ty_of(ty, false);
                        for _ in c.tree.children(x).filter(|&d| c.kind(d) == Declarator) {
                            self.note(t.clone());
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

pub(crate) struct UnionFind(pub(crate) Vec<usize>);

impl UnionFind {
    pub(crate) fn find(&mut self, x: usize) -> usize {
        if self.0[x] == x {
            return x;
        }
        let r = self.find(self.0[x]);
        self.0[x] = r;
        r
    }
    pub(crate) fn join(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        self.0[ra] = rb;
    }
}

pub(crate) fn class_metrics(c: &Cst, classes: &Classes) -> Vec<ClassMetrics> {
    let g = globals(c);
    let mut out = Vec::new();
    for i in classes.user() {
        let cls = &classes.list[i];
        let mut ancestry = Vec::new();
        for (name, k) in classes.chain(i) {
            match k {
                Some(k) if !classes.list[k].library => ancestry.push(name),
                _ => break,
            }
        }
        let env0 = Env::new(c, classes, Some(i), None, &g);
        let mut refs = Usage {
            env: &env0,
            own_fields: BTreeSet::new(),
            own_calls: BTreeSet::new(),
            foreign: BTreeSet::new(),
            types: BTreeSet::new(),
            type_class: BTreeMap::new(),
        };
        for p in &cls.props {
            refs.note(env0.ty_of(p.ty, p.array));
        }
        let mut types = std::mem::take(&mut refs.types);
        let mut type_class = std::mem::take(&mut refs.type_class);
        let mut wmc = 0;
        let mut foreign: BTreeSet<String> = BTreeSet::new();
        // Methods that use own fields or call own methods.
        let mut uses: Vec<(String, BTreeSet<String>, BTreeSet<String>)> = Vec::new();
        for m in &cls.methods {
            wmc += routine_metrics(c, m.body).cyclomatic;
            let env = Env::new(c, classes, Some(i), Some(m.decl), &g);
            let mut u = Usage {
                env: &env,
                own_fields: BTreeSet::new(),
                own_calls: BTreeSet::new(),
                foreign: BTreeSet::new(),
                types: BTreeSet::new(),
                type_class: BTreeMap::new(),
            };
            for p in &m.ports {
                u.note(env.ty_of(p.ty, false));
            }
            if let Some(ret) = c.tree.child(m.decl, ReturnType) {
                u.note(env.ty_of(c.tree.child(ret, DataType), false));
            }
            if let Some(b) = m.body {
                u.walk(b);
            }
            foreign.extend(u.foreign);
            types.extend(u.types);
            for (k, v) in u.type_class {
                type_class.entry(k).or_insert(v);
            }
            if m.name != "new" && (!u.own_fields.is_empty() || !u.own_calls.is_empty()) {
                uses.push((m.name.clone(), u.own_fields, u.own_calls));
            }
        }
        for a in &ancestry {
            types.remove(a);
        }
        let coupled_to: Vec<String> = types.into_iter().collect();
        let coupled = coupled_to.iter().filter_map(|n| type_class.get(n).copied()).collect();
        let n = uses.len();
        let mut tcc = 1.0;
        if n >= 2 {
            let (mut pairs, mut connected) = (0usize, 0usize);
            for a in 0..n {
                for b in a + 1..n {
                    pairs += 1;
                    if uses[a].1.iter().any(|f| uses[b].1.contains(f)) {
                        connected += 1;
                    }
                }
            }
            if pairs > 0 {
                tcc = connected as f64 / pairs as f64;
            }
        }
        let mut uf = UnionFind((0..n).collect());
        let index: HashMap<&str, usize> = uses.iter().enumerate().map(|(k, u)| (u.0.as_str(), k)).collect();
        for a in 0..n {
            for b in a + 1..n {
                if uses[a].1.iter().any(|f| uses[b].1.contains(f)) {
                    uf.join(a, b);
                }
            }
            for callee in &uses[a].2 {
                if let Some(&b) = index.get(callee.as_str()) {
                    uf.join(a, b);
                }
            }
        }
        let mut groups: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        for (k, u) in uses.iter().enumerate().take(n) {
            let r = uf.find(k);
            groups.entry(r).or_default().push(u.0.clone());
        }
        out.push(ClassMetrics {
            cls: i,
            wmc,
            atfd: foreign.len(),
            tcc,
            lcom_groups: groups.into_values().collect(),
            cbo: coupled_to.len(),
            coupled_to,
            coupled,
            dit: ancestry.len(),
            ancestry,
            fields: cls.props.len(),
            methods: cls.methods.len(),
        });
    }
    out
}

pub(crate) fn each_class(c: &Cst, mut f: impl FnMut(&Classes, &ClassMetrics)) {
    let classes = Classes::new(c);
    let all = crate::shared::get("class_metrics", c.tree_key(), || class_metrics(c, &classes));
    for cm in all.iter() {
        f(&classes, cm);
    }
}
