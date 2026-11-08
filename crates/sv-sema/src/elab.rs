//! Elaboration (builder phase N4): the instance tree of a design. From each
//! top module down, every instance (arrays expanded), every generate block
//! that is taken (loops unrolled, `if` and `case` decided) and every value
//! parameter with the value it has in that instance.
//!
//! Lines are those of `uvmcheck_sym_dump --hier`:
//!
//! ```text
//! <path> instance <name> : <definition>
//! <path> generate <name>
//! <path> parameter <name> = <value>
//! ```
//!
//! Values come from the constant evaluator ([`Builder::eval`]); the
//! parameters of the instance being elaborated and the genvars around are
//! in `Builder::inst`, which name lookups read after a function's locals.
//! A parameter whose value cannot be evaluated is left out, and so is
//! anything under a generate condition or loop bound that cannot be.

use super::packed::Ty;
use super::types::CVal;
use super::*;

/// Instances deeper than this are not elaborated (recursive designs).
const MAX_DEPTH: usize = 64;
/// Generate loop iterations allowed in one loop.
const MAX_ITER: i128 = 1 << 16;

/// Values of the parameters and genvars visible at a point of an instance.
/// `None`: one with no value (unknown, or not integral).
/// Parameter and genvar values in scope: a block's own, then its parents'.
/// Cloning shares; a block adds a small scope of its own instead of
/// copying every value above it.
#[derive(Clone, Default)]
pub(crate) struct Vals(Rc<Scope>);

#[derive(Clone, Default)]
struct Scope {
    /// `None`: removed here (hides a parent's value).
    own: crate::fx::FxMap<String, Option<Option<CVal>>>,
    parent: Option<Vals>,
    /// Lookups found here (a loop checks whether its body read the genvar).
    hits: std::cell::Cell<u32>,
}

impl Vals {
    /// A new scope below this one.
    fn child(&self) -> Vals {
        Vals(Rc::new(Scope { own: Default::default(), parent: Some(self.clone()), hits: Default::default() }))
    }

    pub(crate) fn get(&self, k: &str) -> Option<&Option<CVal>> {
        let mut s = &*self.0;
        loop {
            if let Some(v) = s.own.get(k) {
                s.hits.set(s.hits.get().wrapping_add(1));
                return v.as_ref();
            }
            s = &*s.parent.as_ref()?.0;
        }
    }

    /// This scope's own values, printed in full (to compare instances).
    pub(crate) fn pairs(&self) -> Vec<(String, String)> {
        self.0.own.iter().map(|(k, v)| (k.clone(), format!("{v:?}"))).collect()
    }

    fn insert(&mut self, k: String, v: Option<CVal>) {
        Rc::make_mut(&mut self.0).own.insert(k, Some(v));
    }

    fn remove(&mut self, k: &str) {
        Rc::make_mut(&mut self.0).own.insert(k.to_string(), None);
    }
}

/// An override of a parameter: its expression where the instance (or
/// defparam) is, and its value when integral.
#[derive(Clone)]
struct Over {
    val: Option<CVal>,
    node: NodeRef,
    scope: usize,
    vals: Vals,
    env: Env,
}

type Overrides = HashMap<String, Over>;

struct Def {
    decl: NodeRef,
    scope: usize,
}

pub(crate) struct Elab<'b, 't> {
    b: &'b Builder<'t>,
    defs: HashMap<String, Def>,
    /// `bind` directives by target: a definition's name or an instance path.
    binds: Vec<(String, Option<Vec<String>>, NodeRef)>,
    /// Defparams of the instances being elaborated: the target path from
    /// here and the value (evaluated where the defparam is).
    defparams: Vec<Vec<(Vec<String>, Over)>>,
    /// Type parameters of the instances being elaborated.
    envs: Vec<Env>,
    pub out: Vec<String>,
    /// Errors only elaboration finds, once per node.
    pub diags: Vec<crate::names::Diag>,
    checked: HashSet<NodeRef>,
    /// `-top` names; empty: every module nothing instantiates.
    pub top: Vec<String>,
    /// Generate items some instance takes, the branches of the `if` and
    /// `case` generates decided in some instance, and the definitions
    /// elaborated.
    taken: HashSet<NodeRef>,
    considered: HashSet<NodeRef>,
    reached: HashSet<NodeRef>,
    /// What no instance elaborates: generate items no instance takes, and
    /// definitions that need parameter values and that nothing instantiates.
    pub skipped: Vec<NodeRef>,
    /// The copies slang builds of each definition body and generate item:
    /// (all, instantiated). A branch no condition takes is still built,
    /// uninstantiated, with its loops unrolled; instances there are not
    /// elaborated. Instances a `bind` adds are left out (with all under
    /// them), as the engine's walk does not reach them.
    pub copies: HashMap<NodeRef, (usize, usize)>,
    /// Depth of uninstantiated blocks and of bound instances being walked.
    uninst: usize,
    bound: usize,
    /// While above zero, `copy` also logs what it counts, so that a loop
    /// can repeat one iteration's copies instead of walking them again.
    recording: usize,
    rec: Vec<(NodeRef, usize)>,
    /// Each instance elaborated (not bound): its definition, the values and
    /// type parameters its body sees, and the copies counted inside it.
    pub(crate) inst_log: Vec<InstLog>,
    /// The instances being walked, as indexes into `inst_log`.
    inst_stack: Vec<usize>,
}

/// One elaborated instance (see `Elab::inst_log`).
pub(crate) struct InstLog {
    pub decl: NodeRef,
    pub vals: Vals,
    pub env: Env,
    pub copies: HashMap<NodeRef, usize>,
}

impl<'b, 't> Elab<'b, 't> {
    pub fn new(b: &'b Builder<'t>) -> Self {
        let mut defs = HashMap::default();
        for (ti, t) in b.trees.iter().enumerate() {
            for n in t.preorder() {
                if matches!(t.kind(n), ModuleDecl | InterfaceDecl | ProgramDecl) {
                    let r = NodeRef { tree: ti, node: n };
                    let Some((name, _)) = b.name_of(r) else { continue };
                    let Some(&scope) = b.node_scope.get(&r) else { continue };
                    defs.entry(name).or_insert(Def { decl: r, scope });
                }
            }
        }
        let mut binds = Vec::new();
        for (ti, t) in b.trees.iter().enumerate() {
            for n in t.preorder() {
                if t.kind(n) != BindDirective {
                    continue;
                }
                let r = NodeRef { tree: ti, node: n };
                let kids = b.children(r);
                let Some(&target) = kids.first() else { continue };
                if b.kind(target) == DataType {
                    continue;
                }
                let target = b.toks_of(target).concat();
                // `bind m : a.b, c ...`: those instances of m only.
                let list = b.own_tokens(r).contains(&":").then(|| {
                    kids.iter()
                        .skip(1)
                        .take_while(|k| b.kind(**k) != DataType)
                        .map(|k| b.toks_of(*k).concat())
                        .collect()
                });
                binds.push((target, list, r));
            }
        }
        Elab {
            b,
            defs,
            binds,
            defparams: Vec::new(),
            envs: vec![Rc::new(HashMap::default())],
            out: Vec::new(),
            diags: Vec::new(),
            checked: HashSet::default(),
            top: Vec::new(),
            taken: HashSet::default(),
            considered: HashSet::default(),
            reached: HashSet::default(),
            skipped: Vec::new(),
            copies: HashMap::default(),
            uninst: 0,
            bound: 0,
            recording: 0,
            rec: Vec::new(),
            inst_log: Vec::new(),
            inst_stack: Vec::new(),
        }
    }

    /// Counts one copy of a body or generate item.
    fn copy(&mut self, n: NodeRef) {
        if self.bound > 0 {
            return;
        }
        let e = self.copies.entry(n).or_default();
        e.0 += 1;
        if self.uninst == 0 {
            e.1 += 1;
        }
        if let Some(&i) = self.inst_stack.last() {
            *self.inst_log[i].copies.entry(n).or_default() += 1;
        }
        if self.recording > 0 {
            self.rec.push((n, 1));
        }
    }

    /// Counts `times` more of copies an uninstantiated walk logged.
    fn repeat(&mut self, log: &[(NodeRef, usize)], times: usize) {
        if times == 0 {
            return;
        }
        for &(n, c) in log {
            self.copies.entry(n).or_default().0 += c * times;
            if let Some(&i) = self.inst_stack.last() {
                *self.inst_log[i].copies.entry(n).or_default() += c * times;
            }
            if self.recording > 0 {
                self.rec.push((n, c * times));
            }
        }
    }

    /// Modules no design unit instantiates (and that need no parameter value).
    pub fn tops(&self) -> Vec<String> {
        if !self.top.is_empty() {
            let mut tops: Vec<String> = self.top.iter().filter(|t| self.defs.contains_key(*t)).cloned().collect();
            tops.sort();
            tops.dedup();
            return tops;
        }
        let b = self.b;
        let mut used = HashSet::default();
        for t in b.trees {
            for n in t.preorder() {
                if matches!(t.kind(n), Instantiation | BindDirective) {
                    if let Some(dt) = t.child(n, DataType) {
                        if let Some(c) = t.token_bounds(dt) {
                            used.insert(t.tokens[c.0 as usize].text.to_string());
                        }
                    }
                }
            }
        }
        let mut tops: Vec<String> = self
            .defs
            .iter()
            .filter(|(n, d)| {
                !used.contains(*n)
                    && !b.trees[d.decl.tree].library
                    && matches!(b.kind(d.decl), ModuleDecl | ProgramDecl)
                    && !self.needs_params(d.decl)
            })
            .map(|(n, _)| n.clone())
            .collect();
        tops.sort();
        tops
    }

    /// A top-level module's interface ports have nothing to connect to
    /// (23.3.3.4).
    fn interface_ports(&mut self, top: &str) {
        let b = self.b;
        let Some(def) = self.defs.get(top) else { return };
        let decl = def.decl;
        let Some(pl) = b.child(decl, Header).and_then(|h| b.child(h, PortList)) else { return };
        let is_iface = |name: &str| self.defs.get(name).is_some_and(|d| b.kind(d.decl) == InterfaceDecl);
        let body = b.children(decl);
        for port in b.children(pl).into_iter().filter(|p| b.kind(*p) == Port) {
            let own = b.own_tokens(port);
            let Some(d) = b.child(port, Declarator) else { continue };
            let Some((name, _)) = b.name_of(d) else { continue };
            let dtype = b.child(port, DataType).map(|t| b.toks_of(t));
            let iface = match own.first().copied() {
                Some("interface") => true,
                Some(n) if own.get(1) == Some(&".") => is_iface(n),
                None => match dtype {
                    Some(t) => t.len() == 1 && is_iface(t[0]),
                    // Non-ANSI: the body declares it.
                    None => body.iter().filter(|c| b.kind(**c) == DataDecl).any(|c| {
                        let named = b
                            .children(*c)
                            .into_iter()
                            .any(|x| b.kind(x) == Declarator && b.name_of(x).is_some_and(|(n, _)| n == name));
                        let t = b.child(*c, DataType).map(|t| b.toks_of(t)).unwrap_or_default();
                        named && !t.is_empty() && is_iface(t[0]) && (t.len() == 1 || t.get(1) == Some(&"."))
                    }),
                },
                _ => false,
            };
            if iface {
                self.diags.push(crate::names::Diag {
                    tree: d.tree,
                    node: d.node,
                    message: format!("top-level module '{top}' has unconnected interface port '{name}'"),
                });
            }
        }
    }

    /// A header parameter without a default (a top cannot have one).
    fn needs_params(&self, decl: NodeRef) -> bool {
        let b = self.b;
        let Some(pl) = b.child(decl, Header).and_then(|h| b.child(h, ParamList)) else { return false };
        b.children(pl).into_iter().filter(|p| b.kind(*p) == ParamDecl).any(|p| {
            b.children(p)
                .into_iter()
                .filter(|d| b.kind(*d) == Declarator)
                .any(|d| b.children(d).into_iter().all(|c| matches!(b.kind(c), Name | Dim)))
        })
    }

    pub fn run(&mut self) {
        for top in self.tops() {
            self.interface_ports(&top);
            self.out.push(format!("$root instance {top} : {top}"));
            self.instance(&top, &top, &Overrides::default(), Vec::new(), Rc::new(HashMap::default()), 0);
        }
        self.out.sort();
        let mut skipped: Vec<NodeRef> = self.considered.difference(&self.taken).copied().collect();
        for d in self.defs.values() {
            if !self.reached.contains(&d.decl) && self.needs_params(d.decl) {
                skipped.push(d.decl);
            }
        }
        skipped.sort_by_key(|r| (r.tree, r.node.0));
        self.skipped = skipped;
    }

    /// The items of an `if` (with its `else if` chain) or `case` generate.
    fn branches(&self, c: NodeRef, out: &mut Vec<NodeRef>) {
        let b = self.b;
        let kids = b.children(c);
        if b.kind(c) == IfGenerate {
            out.extend(kids.get(1).copied());
            if let Some(e) = kids.iter().find(|k| b.kind(**k) == ElseClause) {
                if let Some(&item) = b.children(*e).first() {
                    if b.kind(item) == IfGenerate {
                        self.branches(item, out);
                    } else {
                        out.push(item);
                    }
                }
            }
            return;
        }
        let t = b.tree(c);
        let mut after_colon = false;
        for ch in &t.node(c.node).children {
            match *ch {
                sv_syntax::Child::Token(i) => after_colon |= &*t.tokens[i as usize].text == ":",
                sv_syntax::Child::Node(n) if after_colon => {
                    out.push(NodeRef { tree: c.tree, node: n });
                    after_colon = false;
                }
                _ => {}
            }
        }
    }

    /// Every condition, selector and label of the construct evaluates.
    fn decided(&self, c: NodeRef, scope: usize, vals: &Vals) -> bool {
        let b = self.b;
        let kids = b.children(c);
        if b.kind(c) == IfGenerate {
            let Some(cond) = kids.first().and_then(|k| self.eval(*k, scope, vals)) else { return false };
            if cond.is_true() {
                return true;
            }
            return match kids.iter().find(|k| b.kind(**k) == ElseClause).and_then(|e| b.children(*e).first().copied()) {
                Some(i) if b.kind(i) == IfGenerate => self.decided(i, scope, vals),
                _ => true,
            };
        }
        let mut items = Vec::new();
        self.branches(c, &mut items);
        kids.iter().filter(|k| !items.contains(k)).all(|k| self.eval(*k, scope, vals).is_some())
    }

    /// One instance of definition `def` at `path`.
    fn instance(
        &mut self,
        def: &str,
        path: &str,
        over: &Overrides,
        pend: Vec<(Vec<String>, Over)>,
        env: Env,
        depth: usize,
    ) {
        if depth > MAX_DEPTH {
            return;
        }
        self.defparams.push(pend);
        self.envs.push(env);
        self.instance_body(def, path, over, depth);
        self.envs.pop();
        self.defparams.pop();
    }

    fn env(&self) -> Env {
        self.envs.last().cloned().unwrap_or_default()
    }

    fn instance_body(&mut self, def: &str, path: &str, over: &Overrides, depth: usize) {
        let Some(d) = self.defs.get(def) else { return };
        let (decl, scope) = (d.decl, d.scope);
        self.reached.insert(decl);
        let logged = self.bound == 0;
        if logged {
            self.inst_log.push(InstLog { decl, vals: Vals::default(), env: self.env(), copies: HashMap::default() });
            self.inst_stack.push(self.inst_log.len() - 1);
        }
        self.copy(decl);
        let b = self.b;
        let mut vals = Vals::default();
        // Header parameters: `#(parameter A = 1, int B = 2, localparam C = 3)`.
        let header = b.child(decl, Header).and_then(|h| b.child(h, ParamList));
        if let Some(pl) = header {
            let (mut dt, mut local, mut ty) = (None, false, false);
            for p in b.children(pl) {
                if b.kind(p) != ParamDecl {
                    continue;
                }
                let own = b.own_tokens(p);
                if own.contains(&"type") {
                    ty = true;
                    local = own.contains(&"localparam");
                    continue;
                }
                let kw = own.iter().any(|t| matches!(*t, "parameter" | "localparam"));
                if kw {
                    local = own.contains(&"localparam");
                }
                if kw || b.child(p, DataType).is_some() {
                    dt = b.child(p, DataType);
                    ty = false;
                }
                if ty {
                    continue;
                }
                self.params(p, dt, !local, path, scope, over, &mut vals);
            }
        }
        let body: Vec<NodeRef> = b.children(decl);
        self.defparams_of(&body, scope, over, header.is_none(), &vals);
        self.items(&body, path, scope, over, header.is_none(), &mut vals, depth);
        self.check_selects(decl, scope, &vals);
        if logged {
            if let Some(i) = self.inst_stack.pop() {
                self.inst_log[i].vals = vals.clone();
            }
        }
        // Instances bound into this one.
        let binds: Vec<NodeRef> = self
            .binds
            .iter()
            .filter(|(target, list, _)| match list {
                _ if target == path || path.ends_with(&format!(".{target}")) => true,
                None => target == def,
                Some(l) => target == def && l.iter().any(|i| i == path || path.ends_with(&format!(".{i}"))),
            })
            .map(|x| x.2)
            .collect();
        for bd in binds {
            self.bound += 1;
            self.instantiation(bd, path, scope, &vals, depth);
            self.bound -= 1;
        }
    }

    /// The declarators of one parameter declaration.
    #[allow(clippy::too_many_arguments)]
    fn params(
        &mut self,
        p: NodeRef,
        dt: Option<NodeRef>,
        overridable: bool,
        path: &str,
        scope: usize,
        over: &Overrides,
        vals: &mut Vals,
    ) {
        let b = self.b;
        let empty = self.env();
        for d in b.children(p) {
            if b.kind(d) != Declarator {
                continue;
            }
            let Some((name, _)) = b.name_of(d) else { continue };
            *b.inst.borrow_mut() = vals.clone();
            let shape = dt.and_then(|t| b.int_shape(t, scope, &empty, 0));
            let (text, val) = match over.get(&name).filter(|_| overridable) {
                Some(o) => match o.val.clone().filter(|_| shape.is_some() || dt.is_none()) {
                    Some(v) => {
                        let v = match shape {
                            Some((w, s)) => {
                                // Sized by this parameter's type (IEEE 11.6).
                                *b.inst.borrow_mut() = o.vals.clone();
                                let c = b.eval_ctx(o.node, w, s, o.scope, &o.env, 0);
                                *b.inst.borrow_mut() = vals.clone();
                                c.unwrap_or_else(|| v.cast(w, s))
                            }
                            None => v,
                        };
                        (Some(v.show()), Some(v))
                    }
                    // A string, real or assignment pattern: evaluated
                    // there, for this parameter's type.
                    None => {
                        *b.inst.borrow_mut() = o.vals.clone();
                        let val = dt.and_then(|t| b.eval_typed(o.node, Ty::of(t, scope), o.scope, &o.env, 0));
                        let text = match &val {
                            _ if self.uninst > 0 => None,
                            Some(v) => Some(v.show()),
                            None => b.value_text(dt, o.node, o.scope, &o.env),
                        };
                        (text, val)
                    }
                },
                None => {
                    let v = b.children(d).into_iter().find(|c| !matches!(b.kind(*c), Name | Dim));
                    match v {
                        Some(v) => {
                            // The text is only printed where instantiated.
                            let text = if self.uninst == 0 { b.value_text(dt, v, scope, &empty) } else { None };
                            let two = dt.is_some_and(|t| b.two_state(t));
                            let val = match dt {
                                Some(t) if b.kind(v) == AssignPattern => {
                                    b.eval_typed(v, Ty::of(t, scope), scope, &empty, 0)
                                }
                                _ => b.eval_xz(v, scope, &empty, two).map(|c| match shape {
                                    Some((w, s)) if c.w < w => {
                                        let x = b.eval_ctx(v, w, s, scope, &empty, 0).unwrap_or_else(|| c.cast(w, s));
                                        if two {
                                            x.two_state()
                                        } else {
                                            x
                                        }
                                    }
                                    Some((w, s)) => c.cast(w, s),
                                    None => c,
                                }),
                            };
                            (text, val)
                        }
                        None => (None, None),
                    }
                }
            };
            if let Some(t) = text {
                if self.uninst == 0 {
                    self.out.push(format!("{path} parameter {name} = {t}"));
                }
            }
            *b.inst.borrow_mut() = Vals::default();
            vals.insert(name, val);
        }
    }

    /// The items of a module body or generate block.
    #[allow(clippy::too_many_arguments)]
    fn items(
        &mut self,
        items: &[NodeRef],
        path: &str,
        scope: usize,
        over: &Overrides,
        body_params: bool,
        vals: &mut Vals,
        depth: usize,
    ) {
        let mut construct = 0;
        self.walk(items, path, scope, over, body_params, vals, depth, &mut construct);
    }

    #[allow(clippy::too_many_arguments)]
    fn walk(
        &mut self,
        items: &[NodeRef],
        path: &str,
        scope: usize,
        over: &Overrides,
        body_params: bool,
        vals: &mut Vals,
        depth: usize,
        construct: &mut usize,
    ) {
        let b = self.b;
        for &c in items {
            match b.kind(c) {
                ParamDecl => {
                    let own = b.own_tokens(c);
                    if own.contains(&"defparam") {
                        continue;
                    }
                    if own.contains(&"type") {
                        continue;
                    }
                    let overridable = body_params && own.contains(&"parameter");
                    self.params(c, b.child(c, DataType), overridable, path, scope, over, vals);
                }
                GenerateRegion => self.walk(&b.children(c), path, scope, over, body_params, vals, depth, construct),
                Instantiation => self.instantiation(c, path, scope, vals, depth),
                LoopGenerate => {
                    *construct += 1;
                    self.loop_gen(c, path, scope, vals, depth, *construct);
                }
                // A bare `begin ... end` among module items is a generate block.
                GenerateBlock => {
                    *construct += 1;
                    self.block(c, None, path, scope, vals, depth, *construct);
                }
                IfGenerate | CaseGenerate => {
                    *construct += 1;
                    let n = *construct;
                    if self.uninst > 0 {
                        // Not decided: every branch is built.
                        let mut all = Vec::new();
                        self.branches(c, &mut all);
                        for item in all {
                            self.block(item, None, path, scope, vals, depth, n);
                        }
                        continue;
                    }
                    if self.decided(c, scope, vals) {
                        let mut all = Vec::new();
                        self.branches(c, &mut all);
                        self.considered.extend(all);
                    }
                    let mut item = self.choose(c, scope, vals);
                    let mut chain = vec![c];
                    // `if (a) if (b) ...`: a nested construct with no
                    // block of its own belongs to this one.
                    for _ in 0..64 {
                        match item {
                            Some(i) if matches!(b.kind(i), IfGenerate | CaseGenerate) => {
                                chain.push(i);
                                item = self.choose(i, scope, vals)
                            }
                            _ => break,
                        }
                    }
                    if let Some(item) = item {
                        self.taken.insert(item);
                        self.block(item, None, path, scope, vals, depth, n);
                    }
                    // The branches not taken, built uninstantiated.
                    let mut rest = Vec::new();
                    for &k in &chain {
                        self.branches(k, &mut rest);
                    }
                    self.uninst += 1;
                    for r in rest {
                        if Some(r) != item && !chain.contains(&r) {
                            self.block(r, None, path, scope, vals, depth, n);
                        }
                    }
                    self.uninst -= 1;
                }
                _ => {}
            }
        }
    }

    /// The item an `if` or `case` generate takes (`None`: none, or unknown).
    fn choose(&self, c: NodeRef, scope: usize, vals: &Vals) -> Option<NodeRef> {
        let b = self.b;
        let t = b.tree(c);
        if b.kind(c) == IfGenerate {
            let kids = b.children(c);
            let cond = self.eval(*kids.first()?, scope, vals)?;
            if cond.is_true() {
                return kids.get(1).copied();
            }
            let e = kids.into_iter().find(|k| b.kind(*k) == ElseClause)?;
            let item = *b.children(e).first()?;
            // `else if`: the same construct.
            return if b.kind(item) == IfGenerate { self.choose(item, scope, vals) } else { Some(item) };
        }
        // case: `e1, e2 : item` ... `default : item`
        let mut kids = b.children(c).into_iter();
        let sel = self.eval(kids.next()?, scope, vals)?;
        let mut labels: Vec<NodeRef> = Vec::new();
        let mut default = false;
        let mut default_item = None;
        let mut after_colon = false;
        for ch in &t.node(c.node).children {
            match *ch {
                sv_syntax::Child::Token(i) => match &*t.tokens[i as usize].text {
                    ":" => after_colon = true,
                    "default" => default = true,
                    _ => {}
                },
                sv_syntax::Child::Node(n) => {
                    let r = NodeRef { tree: c.tree, node: n };
                    if r.node == b.children(c)[0].node {
                        continue;
                    }
                    if after_colon {
                        if default {
                            default_item = Some(r);
                        } else {
                            for l in &labels {
                                let v = self.eval(*l, scope, vals)?;
                                let w = v.w.max(sel.w);
                                if v.cast(w, v.s && sel.s).v == sel.cast(w, v.s && sel.s).v {
                                    return Some(r);
                                }
                            }
                        }
                        labels.clear();
                        default = false;
                        after_colon = false;
                    } else {
                        labels.push(r);
                    }
                }
            }
        }
        default_item
    }

    fn eval(&self, e: NodeRef, scope: usize, vals: &Vals) -> Option<CVal> {
        *self.b.inst.borrow_mut() = vals.clone();
        self.b.eval(e, scope, &self.env(), 0)
    }

    /// A taken generate item: a block of its own (named, or `genblk<n>`).
    #[allow(clippy::too_many_arguments)]
    fn block(
        &mut self,
        item: NodeRef,
        index: Option<i128>,
        path: &str,
        scope: usize,
        vals: &Vals,
        depth: usize,
        construct: usize,
    ) {
        let b = self.b;
        let items: Vec<NodeRef> = if b.kind(item) == GenerateBlock {
            b.children(item).into_iter().filter(|k| !matches!(b.kind(*k), EndLabel | StmtLabel)).collect()
        } else {
            vec![item]
        };
        if self.uninst > 0 {
            // Nothing under an uninstantiated block prints or is
            // instantiated, so its path is not needed.
            self.copy(item);
            let mut v = vals.child();
            self.items(&items, path, scope, &Overrides::default(), false, &mut v, depth);
            return;
        }
        let label = if b.kind(item) == GenerateBlock { self.label(item) } else { None };
        let mut name = label.unwrap_or_else(|| format!("genblk{construct}"));
        if let Some(i) = index {
            name = format!("{name}[{i}]");
        }
        self.copy(item);
        if self.uninst == 0 {
            self.out.push(format!("{path} generate {name}"));
        }
        let inner = format!("{path}.{name}");
        let mut v = vals.child();
        self.items(&items, &inner, scope, &Overrides::default(), false, &mut v, depth);
    }

    fn label(&self, block: NodeRef) -> Option<String> {
        let b = self.b;
        let l = b.child(block, StmtLabel).or_else(|| b.child(block, EndLabel))?;
        b.own_tokens(l).last().map(|s| s.to_string()).filter(|s| *s != ":")
    }

    /// `for (i = 0; i < N; i++) begin : g ... end`
    fn loop_gen(&mut self, c: NodeRef, path: &str, scope: usize, vals: &Vals, depth: usize, construct: usize) {
        let b = self.b;
        let t = b.tree(c);
        let mut part = 0;
        let mut parts: [Vec<NodeRef>; 4] = Default::default();
        let mut var = None;
        for ch in &t.node(c.node).children {
            match *ch {
                sv_syntax::Child::Token(i) => {
                    if matches!(&*t.tokens[i as usize].text, ";" | ")") {
                        part += 1;
                    }
                }
                sv_syntax::Child::Node(n) => {
                    let r = NodeRef { tree: c.tree, node: n };
                    if part == 0 && b.kind(r) == Name {
                        var = b.own_tokens(r).last().map(|s| s.to_string());
                        continue;
                    }
                    parts[part.min(3)].push(r);
                }
            }
        }
        let Some(var) = var else { return };
        let (Some(&init), Some(&cond), Some(&step), Some(&body)) =
            (parts[0].first(), parts[1].first(), parts[2].first(), parts[3].first())
        else {
            return;
        };
        let mut v = vals.child();
        let Some(mut i) = self.eval(init, scope, &v).map(|x| x.cast(32, true)) else {
            if self.uninst > 0 {
                // Unknown bounds where nothing is instantiated: one copy.
                self.block(body, None, path, scope, &v, depth, construct);
            }
            return;
        };
        let mut same: Option<Vec<(NodeRef, usize)>> = None;
        let mut skipped = 0;
        for k in 0..MAX_ITER {
            *self.b.inst.borrow_mut() = Vals::default();
            v.insert(var.clone(), Some(i.clone()));
            match self.eval(cond, scope, &v) {
                Some(x) if x.is_true() => {}
                // A loop that runs no iteration still has one block,
                // uninstantiated (its names are checked).
                // So does one whose bounds are unknown where nothing is
                // instantiated.
                c if k == 0 && (c.is_some() || self.uninst > 0) => {
                    *self.b.inst.borrow_mut() = Vals::default();
                    v.remove(&var);
                    self.uninst += 1;
                    self.block(body, None, path, scope, &v, depth, construct);
                    self.uninst -= 1;
                    break;
                }
                _ => break,
            }
            // The genvar is a parameter of each block.
            let before = self.out.len();
            if same.is_some() {
                // Uninstantiated, and the body does not read the genvar:
                // every iteration builds the same copies.
                skipped += 1;
            } else if self.uninst > 0 {
                let start = self.rec.len();
                let h0 = v.0.hits.get();
                self.recording += 1;
                self.block(body, Some(i.i()), path, scope, &v, depth, construct);
                self.recording -= 1;
                if v.0.hits.get() == h0 {
                    same = Some(self.rec[start..].to_vec());
                }
                if self.recording == 0 {
                    self.rec.truncate(start);
                }
            } else {
                self.block(body, Some(i.i()), path, scope, &v, depth, construct);
            }
            if self.uninst == 0 && self.out.len() > before {
                let head = self.out[before].clone();
                if let Some(name) = head.strip_prefix(&format!("{path} generate ")) {
                    self.out.push(format!("{path}.{name} parameter {var} = {}", i.show()));
                }
            }
            let Some(n) = self.step(step, &var, scope, &v) else { break };
            if n.v == i.v {
                break;
            }
            i = n;
        }
        if let Some(log) = same {
            self.repeat(&log, skipped);
        }
    }

    /// The genvar's next value: `i++`, `i--`, `i = e`, `i += e` ...
    fn step(&self, s: NodeRef, var: &str, scope: usize, vals: &Vals) -> Option<CVal> {
        let b = self.b;
        let cur = vals.get(var)?.clone()?;
        let kids = b.children(s);
        let op = *b.own_tokens(s).first()?;
        let v = match (b.kind(s), op) {
            (PostfixExpr | PrefixExpr, "++") => cur.bump(1),
            (PostfixExpr | PrefixExpr, "--") => cur.bump(-1),
            (BinaryExpr, "=") => self.eval(*kids.get(1)?, scope, vals)?,
            (BinaryExpr, op) => {
                let r = self.eval(*kids.get(1)?, scope, vals)?;
                types::binary_op(op.strip_suffix('=')?, cur, r)?
            }
            _ => return None,
        };
        Some(v.cast(32, true))
    }

    /// `m #(...) a(...), b[0:1](...);`
    fn instantiation(&mut self, c: NodeRef, path: &str, scope: usize, vals: &Vals, depth: usize) {
        if self.uninst > 0 {
            return;
        }
        let b = self.b;
        let Some(dt) = b.child(c, DataType) else { return };
        let Some(def) = b.toks_of(dt).first().map(|s| s.to_string()) else { return };
        let Some(d) = self.defs.get(&def) else { return };
        let child = d.decl;
        // Overrides, evaluated here.
        let mut over = Overrides::default();
        let mut types: HashMap<String, Bound> = HashMap::default();
        let shared = vals.clone();
        if let Some(pl) = b.child(dt, ParamList) {
            let args: Vec<NodeRef> = match b.child(pl, ArgList) {
                Some(al) => b.children(al).into_iter().filter(|a| b.kind(*a) == Arg).collect(),
                None => b.children(pl),
            };
            let order = self.param_order(child);
            for (k, a) in args.iter().enumerate() {
                let (name, e) = if b.kind(*a) == Arg {
                    let own = b.own_tokens(*a);
                    let e = b.children(*a).into_iter().find(|x| b.kind(*x) != Attribute);
                    if own.first() == Some(&".") {
                        (own.get(1).map(|s| s.to_string()), e)
                    } else {
                        (order.get(k).map(|o| o.0.clone()), e)
                    }
                } else {
                    (order.get(k).map(|o| o.0.clone()), Some(*a))
                };
                let (Some(name), Some(e)) = (name, e) else { continue };
                let is_type = order.iter().any(|(n, t)| *t && *n == name);
                if b.kind(e) == DataType || is_type {
                    types.insert(name, Bound { node: e, scope, env: self.env() });
                    continue;
                }
                let val = self.eval(e, scope, vals);
                over.insert(name, Over { val, node: e, scope, vals: shared.clone(), env: self.env() });
            }
        }
        for inst in b.children(c) {
            if b.kind(inst) != Instance {
                continue;
            }
            let Some((name, _)) = b.name_of(inst) else { continue };
            let dims: Vec<NodeRef> = b.children(inst).into_iter().filter(|k| b.kind(*k) == Dim).collect();
            let mut names = vec![name];
            for dim in dims {
                *b.inst.borrow_mut() = vals.clone();
                let Some((l, r)) = b.dim_range(dim, scope, &self.env()) else { return };
                if (l - r).abs() > MAX_ITER {
                    return;
                }
                let idx: Vec<i128> = if l >= r { (r..=l).collect() } else { (l..=r).collect() };
                names = names.iter().flat_map(|n| idx.iter().map(move |i| format!("{n}[{i}]"))).collect();
            }
            for n in names {
                self.out.push(format!("{path} instance {n} : {def}"));
                // Defparams through this instance: the last name is ours.
                let mut over = over.clone();
                let mut pend = Vec::new();
                for (segs, v) in self.defparams.last().into_iter().flatten() {
                    if segs.first() != Some(&n) {
                        continue;
                    }
                    match &segs[1..] {
                        [p] => {
                            over.insert(p.clone(), v.clone());
                        }
                        rest => pend.push((rest.to_vec(), v.clone())),
                    }
                }
                let env = Rc::new(types.clone());
                self.instance(&def, &format!("{path}.{n}"), &over, pend, env, depth + 1);
            }
        }
    }

    /// `a[i +: w]` with a constant width that is not positive (11.5.1).
    /// Only in the module's own items: not in subroutines, classes or
    /// generate blocks, where a name may be something else.
    fn check_selects(&mut self, decl: NodeRef, scope: usize, vals: &Vals) {
        let b = self.b;
        let mut stack = b.children(decl);
        while let Some(n) = stack.pop() {
            match b.kind(n) {
                FunctionDecl | TaskDecl | ClassDecl | GenerateBlock | LoopGenerate | IfGenerate | CaseGenerate
                | ModuleDecl | InterfaceDecl | ProgramDecl | CovergroupDecl => continue,
                SelectExpr if !self.checked.contains(&n) => {
                    let own = b.own_tokens(n);
                    if own.iter().any(|t| matches!(*t, "+:" | "-:")) {
                        if let Some(&w) = b.children(n).last() {
                            if let Some(v) = self.eval(w, scope, vals) {
                                if v.i() <= 0 {
                                    self.checked.insert(n);
                                    self.diags.push(crate::names::Diag {
                                        tree: n.tree,
                                        node: w.node,
                                        message: format!("indexed part-select width must be positive, not {}", v.i()),
                                    });
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            stack.extend(b.children(n));
        }
    }

    /// The defparams of a body, evaluated with its parameters' values (a
    /// defparam may come before or after the instance it changes).
    fn defparams_of(&mut self, body: &[NodeRef], scope: usize, over: &Overrides, body_params: bool, vals: &Vals) {
        let b = self.b;
        let mut items = Vec::new();
        let mut stack: Vec<NodeRef> = body.iter().rev().copied().collect();
        while let Some(x) = stack.pop() {
            match b.kind(x) {
                GenerateRegion => stack.extend(b.children(x).into_iter().rev()),
                ParamDecl => items.push(x),
                _ => {}
            }
        }
        if !items.iter().any(|p| b.own_tokens(*p).contains(&"defparam")) {
            return;
        }
        let mut v = vals.child();
        let saved = std::mem::take(&mut self.out);
        for &p in &items {
            let own = b.own_tokens(p);
            if !own.contains(&"defparam") && !own.contains(&"type") {
                let overridable = body_params && own.contains(&"parameter");
                self.params(p, b.child(p, DataType), overridable, "", scope, over, &mut v);
            }
        }
        self.out = saved;
        for &p in &items {
            if b.own_tokens(p).contains(&"defparam") {
                self.defparam(p, scope, &v);
            }
        }
    }

    /// `defparam a.b.P = e;`
    fn defparam(&mut self, c: NodeRef, scope: usize, vals: &Vals) {
        let b = self.b;
        let kids = b.children(c);
        let (Some(&target), Some(&e)) = (kids.first(), kids.get(1)) else { return };
        let segs: Vec<String> = b.toks_of(target).split(|t| *t == ".").map(|p| p.concat()).collect();
        if segs.len() < 2 {
            return;
        }
        let val = self.eval(e, scope, vals);
        let o = Over { val, node: e, scope, vals: vals.clone(), env: self.env() };
        if let Some(d) = self.defparams.last_mut() {
            d.push((segs, o));
        }
    }

    /// The names positional overrides bind, in order: the header's
    /// parameters, or the body's when there is no header list.
    fn param_order(&self, decl: NodeRef) -> Vec<(String, bool)> {
        let b = self.b;
        let mut out = Vec::new();
        let header = b.child(decl, Header).and_then(|h| b.child(h, ParamList));
        let decls: Vec<NodeRef> = match header {
            Some(pl) => b.children(pl),
            None => {
                let mut v = Vec::new();
                let mut stack = b.children(decl);
                stack.reverse();
                while let Some(x) = stack.pop() {
                    match b.kind(x) {
                        GenerateRegion => {
                            let mut k = b.children(x);
                            k.reverse();
                            stack.extend(k);
                        }
                        ParamDecl if b.own_tokens(x).contains(&"parameter") => v.push(x),
                        _ => {}
                    }
                }
                v
            }
        };
        let (mut local, mut ty) = (false, false);
        for p in decls {
            if b.kind(p) != ParamDecl {
                continue;
            }
            let own = b.own_tokens(p);
            if own.contains(&"localparam") {
                local = true;
            } else if own.contains(&"parameter") {
                local = false;
            }
            if own.contains(&"type") {
                ty = true;
            } else if own.iter().any(|t| matches!(*t, "parameter" | "localparam")) || b.child(p, DataType).is_some() {
                ty = false;
            }
            if local {
                continue;
            }
            if let Some(n) = b.child(p, Name) {
                if let Some(s) = b.own_tokens(n).last() {
                    out.push((s.to_string(), ty));
                }
            }
            for d in b.children(p) {
                if b.kind(d) == Declarator {
                    if let Some((n, _)) = b.name_of(d) {
                        out.push((n, ty));
                    }
                }
            }
        }
        out
    }
}
