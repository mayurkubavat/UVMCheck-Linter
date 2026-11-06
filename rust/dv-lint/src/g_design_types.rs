//! Group D: expression types over the class index (what slang's
//! `Expression::type` gives the C++ rules), class metrics
//! (`metrics::classMetrics`) and the rules that read them.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use sv_syntax::{NodeId, SyntaxKind};

use crate::g_design::*;
use crate::g_design_class::*;
use crate::g_design_metrics::routine_metrics;
use crate::syntax::Cst;
use crate::Check::Syntax;
use crate::{Rule, Severity, Sink};
use SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule {
            id: "SV-DSN-009",
            name: "RepeatedEnumSwitch",
            check: Syntax(repeated_enum_switch),
        },
        Rule {
            id: "SV-DUP-006",
            name: "DataClumps",
            check: Syntax(data_clumps),
        },
        Rule {
            id: "SV-DED-015",
            name: "NeverSetClassParameter",
            check: Syntax(never_set_parameter),
        },
        Rule {
            id: "UVM-ARC-002",
            name: "HierarchyReachThrough",
            check: Syntax(hierarchy_reach_through),
        },
        Rule {
            id: "SV-DSN-002",
            name: "GodClass",
            check: Syntax(god_class),
        },
        Rule {
            id: "SV-DSN-003",
            name: "LowCohesion",
            check: Syntax(low_cohesion),
        },
        Rule {
            id: "SV-DSN-004",
            name: "HighCoupling",
            check: Syntax(high_coupling),
        },
        Rule {
            id: "SV-DSN-005",
            name: "DeepInheritance",
            check: Syntax(deep_inheritance),
        },
        Rule {
            id: "SV-MET-006",
            name: "LargeClass",
            check: Syntax(large_class),
        },
        Rule {
            id: "UVM-ARC-001",
            name: "LayerDependency",
            check: Syntax(layer_dependency),
        },
    ]
}

// ---- types ----

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
    locals: HashMap<String, (Option<NodeId>, bool)>,
    globals: &'a HashMap<String, (Option<NodeId>, bool)>,
}

/// Variables declared outside classes and routines, by name.
pub(crate) fn globals(c: &Cst) -> HashMap<String, (Option<NodeId>, bool)> {
    let mut out = HashMap::new();
    for d in c.nodes(DataDecl) {
        let Some(p) = c.parent(d) else { continue };
        if !matches!(
            c.kind(p),
            SourceFile | PackageDecl | ModuleDecl | InterfaceDecl | ProgramDecl
        ) {
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
        Env {
            c,
            classes,
            class,
            locals,
            globals,
        }
    }

    pub fn ty_of(&self, ty: Option<NodeId>, array: bool) -> Option<Ty> {
        let ty = ty?;
        let name = type_name(self.c, ty)?;
        let class = self.classes.find(&name);
        Some(Ty {
            name: self.classes.resolve_name(&name),
            class,
            array,
        })
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

    /// slang's `Expression::type`, for the expressions rules care about.
    pub fn expr_type(&self, e: NodeId) -> Option<Ty> {
        let c = self.c;
        let e = strip_parens(c, e);
        match c.kind(e) {
            NameExpr => {
                let t = c.toks(e).next().map(|t| c.text_of(t))?;
                if t == "this" {
                    let i = self.class?;
                    return Some(Ty {
                        name: self.classes.list[i].name.clone(),
                        class: Some(i),
                        array: false,
                    });
                }
                if t == "super" {
                    let b = self.classes.base(self.class?)?;
                    return Some(Ty {
                        name: self.classes.list[b].name.clone(),
                        class: Some(b),
                        array: false,
                    });
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
                class.map(|k| Ty {
                    name: self.classes.list[k].name.clone(),
                    class: Some(k),
                    array: false,
                })
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

fn prev_sibling_decl(c: &Cst, d: NodeId) -> Option<NodeId> {
    let p = c.parent(d)?;
    let kids: Vec<NodeId> = c.tree.children(p).collect();
    let i = kids.iter().position(|&x| x == d)?;
    kids[..i]
        .iter()
        .rev()
        .copied()
        .find(|&x| c.kind(x) == DataDecl && c.tree.child(x, DataType).is_some())
}

/// Enum typedef names.
fn enum_types(c: &Cst) -> HashSet<String> {
    let mut out = HashSet::new();
    for t in c.nodes(TypedefDecl) {
        let (Some(nm), Some(ty)) = (c.tree.child(t, Name), c.tree.child(t, DataType)) else {
            continue;
        };
        if c.first_tok(ty).is_some_and(|k| c.text_of(k) == "enum") {
            out.insert(c.tree.bare_text(nm));
        }
    }
    out
}

// ---- SV-DSN-009 ----

fn repeated_enum_switch(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    let g = globals(c);
    let enums = enum_types(c);
    let mut sites: HashMap<String, Vec<(usize, NodeId)>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for i in classes.user() {
        let mut first: BTreeMap<String, NodeId> = BTreeMap::new();
        for m in &classes.list[i].methods {
            let Some(body) = m.body else { continue };
            let env = Env::new(c, &classes, Some(i), Some(m.decl), &g);
            for st in crate::g_design_class::all_stmts(c, body) {
                if stmt(c, st) != Some(St::Case) {
                    continue;
                }
                let Some(sel) = case_parts(c, st).sel else { continue };
                let Some(t) = env.expr_type(sel) else { continue };
                if t.array || !enums.contains(&t.name) {
                    continue;
                }
                first.entry(t.name).or_insert(st);
            }
        }
        for (ty, st) in first {
            let v = sites.entry(ty.clone()).or_default();
            if v.is_empty() {
                order.push(ty);
            }
            v.push((i, st));
        }
    }
    let min = 3;
    for ty in order {
        let v = &sites[&ty];
        if v.len() < min {
            continue;
        }
        let names = v
            .iter()
            .map(|(i, _)| classes.list[*i].name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        for (i, st) in v {
            s.add(
                "SV-DSN-009",
                "RepeatedEnumSwitch",
                Severity::Info,
                format!(
                    "'{}' switches on enum '{ty}', as do {} other classes ({names}). Every new \
                     value needs an edit in each of them; consider moving the per-kind behavior \
                     into the item or a policy class.",
                    classes.list[*i].name,
                    v.len() - 1
                ),
                start(c, *st),
            );
        }
    }
}

// ---- SV-DUP-006 ----

/// slang's `Type::toString()` of a port type, as far as grouping needs.
fn type_text(c: &Cst, ty: Option<NodeId>) -> String {
    match ty {
        Some(t) => c.tree.bare_text(t),
        None => "logic".into(),
    }
}

fn data_clumps(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    let (min_args, min_routines) = (4, 3);
    let mut groups: BTreeMap<String, Vec<(usize, usize)>> = BTreeMap::new();
    for i in classes.user() {
        for (mi, m) in classes.list[i].methods.iter().enumerate() {
            if classes.override_of(i, &m.name).is_some() || m.ports.len() < min_args {
                continue;
            }
            let mut parts: Vec<String> = m
                .ports
                .iter()
                .map(|p| format!("{} {}", type_text(c, p.ty), p.name))
                .collect();
            parts.sort();
            let key: String = parts.iter().map(|p| format!("{p};")).collect();
            groups.entry(key).or_default().push((i, mi));
        }
    }
    for routines in groups.values() {
        if routines.len() < min_routines {
            continue;
        }
        let mut sorted = routines.clone();
        let pos = |&(i, m): &(usize, usize)| {
            let l = c.loc(classes.list[i].methods[m].tok);
            let l = l
                .expansion
                .map(|(f, line)| (f, line, 0))
                .unwrap_or((l.file, l.line, l.column));
            (l, classes.list[i].methods[m].tok)
        };
        sorted.sort_by_key(pos);
        let mut names = String::new();
        for &(i, m) in sorted.iter().skip(1).take(5) {
            if !names.is_empty() {
                names.push_str(", ");
            }
            names.push_str(&format!(
                "{}::{}",
                classes.list[i].name, classes.list[i].methods[m].name
            ));
        }
        if sorted.len() > 6 {
            names.push_str(", ...");
        }
        let (fi, fm) = sorted[0];
        let first = &classes.list[fi].methods[fm];
        s.add(
            "SV-DUP-006",
            "DataClumps",
            Severity::Info,
            format!(
                "'{}' and {} other routine(s) ({names}) take the same {} arguments. Pass one transaction or struct that holds them.",
                first.name,
                sorted.len() - 1,
                first.ports.len()
            ),
            c.loc(first.tok),
        );
    }
}

// ---- SV-DED-015 ----

/// A class's port parameters (not localparams): name and name token.
fn port_params(c: &Cst, cls: NodeId) -> Vec<(String, u32)> {
    let Some(h) = c.tree.child(cls, Header) else {
        return Vec::new();
    };
    let Some(pl) = c.tree.child(h, ParamList) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for p in c.tree.children(pl).filter(|&x| c.kind(x) == ParamDecl) {
        if c.toks(p).any(|t| c.text_of(t) == "localparam") {
            continue;
        }
        // `int W = 8` has a Declarator; `type T = int` names it directly.
        let names: Vec<NodeId> = c
            .tree
            .children(p)
            .filter_map(|x| match c.kind(x) {
                Declarator => c.tree.child(x, Name),
                Name => Some(x),
                _ => None,
            })
            .collect();
        for nm in names {
            if let Some(t) = c.first_tok(nm) {
                out.push((c.text_of(t).to_string(), t));
            }
        }
    }
    out
}

fn never_set_parameter(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    // Generic user classes some non-generic user class uses as a base or a
    // property type.
    let mut generics: Vec<usize> = Vec::new();
    for i in classes.user() {
        let cls = &classes.list[i];
        if cls.generic {
            continue;
        }
        let mut note = |name: &str| {
            if let Some(k) = classes.find(name) {
                if classes.list[k].generic && !classes.list[k].library && !generics.contains(&k) {
                    generics.push(k);
                }
            }
        };
        if let Some(b) = cls.base.as_deref() {
            note(b);
        }
        for p in &cls.props {
            if let Some(n) = p.ty.and_then(|t| type_name(c, t)) {
                note(&n);
            }
        }
    }
    for k in generics {
        let gen = &classes.list[k];
        let params = port_params(c, gen.node);
        let mut set: BTreeMap<String, bool> = params.iter().map(|(n, _)| (n.clone(), false)).collect();
        // Every `gen #(...)` in the tree, as written.
        for ty in c.nodes(DataType) {
            if type_name(c, ty).as_deref() != Some(gen.name.as_str()) {
                continue;
            }
            let Some(pl) = c.tree.child(ty, ParamList) else {
                continue;
            };
            let Some(args) = c.tree.child(pl, ArgList) else {
                continue;
            };
            for (ix, a) in c.tree.children(args).filter(|&x| c.kind(x) == Arg).enumerate() {
                let named = c.toks(a).any(|t| c.text_of(t) == ".");
                let pname = if named {
                    c.toks(a)
                        .find(|&t| {
                            let x = c.text_of(t);
                            x != "." && x != "(" && x != ")"
                        })
                        .map(|t| c.text_of(t).to_string())
                } else {
                    params.get(ix).map(|(n, _)| n.clone())
                };
                // `.W()` leaves it at its default.
                let empty = named && c.tree.children(a).next().is_none();
                if let (Some(p), false) = (pname, empty) {
                    if let Some(v) = set.get_mut(&p) {
                        *v = true;
                    }
                }
            }
        }
        for (name, used) in set {
            if used {
                continue;
            }
            let tok = params.iter().find(|(n, _)| *n == name).map(|(_, t)| *t).unwrap();
            s.add(
                "SV-DED-015",
                "NeverSetClassParameter",
                Severity::Info,
                format!(
                    "Parameter '{name}' of class '{}' is never set by any specialization; every user takes the default. Make it a \
                     localparam, or remove it until a second configuration exists and is tested.",
                    gen.name
                ),
                c.loc(tok),
            );
        }
    }
}

// ---- UVM-ARC-002 ----

fn hierarchy_reach_through(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    let g = globals(c);
    for i in classes.user() {
        let cls = &classes.list[i];
        for m in &cls.methods {
            if m.name == "connect_phase" || m.name == "end_of_elaboration_phase" {
                continue;
            }
            let Some(body) = m.body else { continue };
            let env = Env::new(c, &classes, Some(i), Some(m.decl), &g);
            let is_component = |t: &Option<Ty>| {
                t.as_ref()
                    .is_some_and(|t| !t.array && t.class.is_some_and(|k| classes.role(k).is_component()))
            };
            let mut inner: HashSet<u32> = HashSet::new();
            for top in subtree(c, body) {
                if c.kind(top) != MemberExpr || inner.contains(&top.0) || is_method_ref(c, &env, top) {
                    continue;
                }
                let mut hops = 0;
                let Some(mut e) = c.tree.children(top).next() else {
                    continue;
                };
                loop {
                    let mut x = strip_parens(c, e);
                    while c.kind(x) == SelectExpr {
                        match c.tree.children(x).next() {
                            Some(v) => x = strip_parens(c, v),
                            None => break,
                        }
                    }
                    if is_component(&env.expr_type(e)) {
                        let this = c.kind(x) == NameExpr && c.toks(x).next().is_some_and(|t| c.text_of(t) == "this");
                        if !this {
                            hops += 1;
                        }
                    }
                    if c.kind(x) != MemberExpr {
                        break;
                    }
                    inner.insert(x.0);
                    match c.tree.children(x).next() {
                        Some(v) => e = v,
                        None => break,
                    }
                }
                if hops < 3 {
                    continue;
                }
                let tt = env.expr_type(top);
                let sequencer = tt.is_some_and(|t| {
                    !t.array
                        && (t.class.is_some_and(|k| classes.role(k) == Role::Sequencer)
                            || t.name.starts_with("uvm_sequencer"))
                });
                if sequencer {
                    continue;
                }
                s.add(
                    "UVM-ARC-002",
                    "HierarchyReachThrough",
                    Severity::Warning,
                    format!(
                        "'{}' reaches through {hops} component handles from {}::{}. It breaks as soon as the env is restructured and couples this code to other \
                         components' internals (Law of Demeter). Pass what is needed through config_db, a \
                         config object or a TLM connection.",
                        normalized(c, top),
                        cls.name,
                        m.name
                    ),
                    start(c, top),
                );
            }
        }
    }
}

/// `a.m` as the callee of a call is a method reference, not a member access.
fn is_method_ref(c: &Cst, env: &Env, e: NodeId) -> bool {
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

// ---- class metrics ----

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

struct Usage<'e, 'a, 'c> {
    env: &'e Env<'a, 'c>,
    own_fields: BTreeSet<String>,
    own_calls: BTreeSet<String>,
    foreign: BTreeSet<String>,
    types: BTreeSet<String>,
    type_class: BTreeMap<String, usize>,
}

impl Usage<'_, '_, '_> {
    fn is_own(&self, k: usize) -> bool {
        Some(k) == self.env.class
    }

    fn note(&mut self, t: Option<Ty>) {
        let Some(t) = t else { return };
        let Some(k) = t.class else { return };
        if self.env.classes.list[k].library || self.is_own(k) {
            return;
        }
        let name = self.env.classes.list[k].name.clone();
        self.types.insert(name.clone());
        self.type_class.entry(name).or_insert(k);
    }

    fn walk(&mut self, root: NodeId) {
        let c = self.env.c;
        let classes = self.env.classes;
        let me = self.env.class.unwrap();
        for x in subtree(c, root) {
            match c.kind(x) {
                NameExpr => {
                    // Callees are calls, not values.
                    if c.parent(x)
                        .is_some_and(|p| c.kind(p) == CallExpr && c.tree.children(p).next() == Some(x))
                    {
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
                        Some(k) => Some(Ty {
                            name: classes.list[k].name.clone(),
                            class: Some(k),
                            array: false,
                        }),
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

struct UnionFind(Vec<usize>);

impl UnionFind {
    fn find(&mut self, x: usize) -> usize {
        if self.0[x] == x {
            return x;
        }
        let r = self.find(self.0[x]);
        self.0[x] = r;
        r
    }
    fn join(&mut self, a: usize, b: usize) {
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

fn each_class(c: &Cst, mut f: impl FnMut(&Classes, &ClassMetrics)) {
    let classes = Classes::new(c);
    for cm in class_metrics(c, &classes) {
        f(&classes, &cm);
    }
}

fn god_class(c: &Cst, s: &mut Sink) {
    let (wmc_limit, atfd_limit, tcc_percent) = (47, 5, 33.0);
    each_class(c, |classes, cm| {
        let role = classes.role(cm.cls);
        if role == Role::RegModel {
            return;
        }
        if cm.wmc < wmc_limit || cm.atfd <= atfd_limit || cm.tcc * 100.0 >= tcc_percent {
            return;
        }
        let cls = &classes.list[cm.cls];
        s.add(
            "SV-DSN-002",
            "GodClass",
            Severity::Warning,
            format!(
                "{} '{}' is a god class: weighted method complexity {}, uses {} fields of other classes, cohesion (TCC) {:.2}. \
                 It does several jobs at once; split it, for example into predictor, comparator and \
                 coverage parts.",
                role.name(),
                cls.name,
                cm.wmc,
                cm.atfd,
                cm.tcc
            ),
            c.loc(cls.tok),
        );
    });
}

fn low_cohesion(c: &Cst, s: &mut Sink) {
    let (min_groups, min_size) = (2, 2);
    each_class(c, |classes, cm| {
        let role = classes.role(cm.cls);
        if matches!(role, Role::Config | Role::Item | Role::RegModel) {
            return;
        }
        let groups: Vec<&Vec<String>> = cm.lcom_groups.iter().filter(|g| g.len() >= min_size).collect();
        if groups.len() < min_groups {
            return;
        }
        let desc = groups
            .iter()
            .map(|g| format!("{{{}}}", g.join(", ")))
            .collect::<Vec<_>>()
            .join("; ");
        let cls = &classes.list[cm.cls];
        s.add(
            "SV-DSN-003",
            "LowCohesion",
            Severity::Info,
            format!(
                "{} '{}' has {} groups of methods that share no data: {desc}. Each group is a separate responsibility; consider moving them into their own classes.",
                role.name(),
                cls.name,
                groups.len()
            ),
            c.loc(cls.tok),
        );
    });
}

fn high_coupling(c: &Cst, s: &mut Sink) {
    let limit = 10;
    each_class(c, |classes, cm| {
        let role = classes.role(cm.cls);
        if matches!(role, Role::Env | Role::Test | Role::RegModel) || cm.cbo <= limit {
            return;
        }
        let cls = &classes.list[cm.cls];
        s.add(
            "SV-DSN-004",
            "HighCoupling",
            Severity::Info,
            format!(
                "{} '{}' depends on {} classes (limit {limit}): {}. A change to any of them can break it; depend on the item, the config and \
                 interfaces, and reach the rest through TLM.",
                role.name(),
                cls.name,
                cm.cbo,
                cm.coupled_to.join(", ")
            ),
            c.loc(cls.tok),
        );
    });
}

fn deep_inheritance(c: &Cst, s: &mut Sink) {
    let limit = 3;
    each_class(c, |classes, cm| {
        if cm.dit <= limit {
            return;
        }
        let cls = &classes.list[cm.cls];
        let mut chain = cls.name.clone();
        for a in &cm.ancestry {
            chain.push_str(&format!(" : {a}"));
        }
        if let Some(b) = classes.uvm_base_name(cm.cls) {
            chain.push_str(&format!(" : {b}"));
        }
        s.add(
            "SV-DSN-005",
            "DeepInheritance",
            Severity::Info,
            format!(
                "'{}' has {} user-defined ancestors (limit {limit}): {chain}. It is hard to tell which override of each phase runs; prefer composition or \
                 config objects over another level.",
                cls.name, cm.dit
            ),
            c.loc(cls.tok),
        );
    });
}

fn large_class(c: &Cst, s: &mut Sink) {
    let (max_fields, max_data_fields, max_methods) = (30, 80, 30);
    each_class(c, |classes, cm| {
        let role = classes.role(cm.cls);
        if role == Role::RegModel {
            return;
        }
        let data = matches!(role, Role::Config | Role::Item);
        let field_limit = if data { max_data_fields } else { max_fields };
        let mut what = String::new();
        if cm.fields > field_limit {
            what = format!("{} properties (limit {field_limit})", cm.fields);
        }
        if cm.methods > max_methods {
            if !what.is_empty() {
                what.push_str(" and ");
            }
            what.push_str(&format!("{} methods (limit {max_methods})", cm.methods));
        }
        if what.is_empty() {
            return;
        }
        let cls = &classes.list[cm.cls];
        s.add(
            "SV-MET-006",
            "LargeClass",
            Severity::Info,
            format!(
                "{} '{}' has {what}. Large classes are hard to review and usually hold several responsibilities; split \
                 out sub-configs, helpers or separate components.",
                role.name(),
                cls.name
            ),
            c.loc(cls.tok),
        );
    });
}

fn layer_of(r: Role) -> Option<usize> {
    match r {
        Role::Item => Some(0),
        Role::Sequence => Some(1),
        Role::Driver | Role::Monitor | Role::Sequencer | Role::Agent | Role::Subscriber => Some(2),
        Role::Scoreboard | Role::Env => Some(3),
        Role::Test => Some(4),
        _ => None,
    }
}

fn layer_name(l: usize) -> &'static str {
    ["sequence item", "sequence", "agent", "env", "test"]
        .get(l)
        .copied()
        .unwrap_or("?")
}

fn layer_dependency(c: &Cst, s: &mut Sink) {
    each_class(c, |classes, cm| {
        let role = classes.role(cm.cls);
        let Some(mine) = layer_of(role) else { return };
        let cls = &classes.list[cm.cls];
        for &dep in &cm.coupled {
            let dr = classes.role(dep);
            let Some(theirs) = layer_of(dr) else { continue };
            if theirs <= mine {
                continue;
            }
            if role == Role::Sequence && dr == Role::Sequencer {
                continue;
            }
            s.add(
                "UVM-ARC-001",
                "LayerDependency",
                Severity::Warning,
                format!(
                    "{} '{}' ({} layer) depends on {} '{}' ({} layer). Lower layers must not know about the \
                     layers that use them, or the {} \
                     code cannot be reused in another env. Pass data down through config objects, \
                     TLM ports or callbacks instead.",
                    role.name(),
                    cls.name,
                    layer_name(mine),
                    dr.name(),
                    classes.list[dep].name,
                    layer_name(theirs),
                    layer_name(mine)
                ),
                c.loc(cls.tok),
            );
        }
    });
}
