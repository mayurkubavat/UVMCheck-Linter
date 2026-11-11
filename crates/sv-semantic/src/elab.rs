//! Elaboration (builder phase N4): the instance tree of a design. From each
//! top module down, every instance (arrays expanded), every generate block
//! that is taken (loops unrolled, `if` and `case` decided) and every value
//! parameter with the value it has in that instance, plus the arena-backed
//! [`crate::elab_ir::ElaboratedDesign`] object model.
//!
//! Lines in [`Elab::out`] are those of `uvmcheck_sym_dump --hier`:
//!
//! ```text
//! <path> instance <name> : <definition>
//! <path> generate <name>
//! <path> parameter <name> = <value>
//! ```

use super::packed::Ty;
use super::types::CVal;
use super::*;
use crate::elab_ir::{
    ElabItem, ElabItemKind, ElaboratedDesign, Instance as DesignInstance, InstanceId, PackageScope, PortConn, PortDir,
    RtlType, Symbol as DesignSymbol, SymbolId, SymbolKind,
};

/// Instances deeper than this are not elaborated (recursive designs).
const MAX_DEPTH: usize = 64;
/// Generate loop iterations allowed in one loop.
const MAX_ITER: i128 = 1 << 16;

const NET_KEYWORDS: &[&str] = &[
    "wire",
    "tri",
    "wand",
    "wor",
    "triand",
    "trior",
    "tri0",
    "tri1",
    "trireg",
    "supply0",
    "supply1",
    "uwire",
    "interconnect",
];

const DIR_KEYWORDS: &[&str] = &["input", "output", "inout", "ref"];

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

    /// Active constant values visible here (innermost binding wins).
    pub(crate) fn active_consts(&self) -> Vec<(String, CVal)> {
        let mut seen = HashSet::default();
        let mut out = Vec::new();
        let mut cur = Some(&*self.0);
        while let Some(s) = cur {
            for (k, v) in &s.own {
                if seen.insert(k.clone()) {
                    if let Some(Some(cv)) = v {
                        out.push((k.clone(), cv.clone()));
                    }
                }
            }
            cur = s.parent.as_ref().map(|p| &*p.0);
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    fn insert(&mut self, k: String, v: Option<CVal>) {
        Rc::make_mut(&mut self.0).own.insert(k, Some(v));
    }

    fn remove(&mut self, k: &str) {
        Rc::make_mut(&mut self.0).own.insert(k.to_string(), None);
    }
}

/// An override of a parameter: its expression where the instance (or
/// defparam) is, or a CLI `-G` value string, and its value when integral.
#[derive(Clone)]
struct Over {
    val: Option<CVal>,
    node: NodeRef,
    scope: usize,
    vals: Vals,
    env: Env,
    cli_raw: Option<String>,
    cli_indices: Vec<usize>,
}

type Overrides = HashMap<String, Over>;

/// A `bind`: its target (a definition or instance path), the instances
/// listed after `:` (if any), the directive, and the definition holding it.
type Bind = (String, Option<Vec<String>>, NodeRef, Option<NodeRef>);

struct Def {
    decl: NodeRef,
    scope: usize,
}

struct NonAnsiPortEntry {
    dir: PortDir,
    dir_dt: Option<NodeRef>,
    decl_dt: Option<NodeRef>,
    decl_item: Option<NodeRef>,
    has_net_kw: bool,
    has_var_kw: bool,
    init_node: Option<NodeRef>,
}

impl Default for NonAnsiPortEntry {
    fn default() -> Self {
        Self {
            dir: PortDir::Inout,
            dir_dt: None,
            decl_dt: None,
            decl_item: None,
            has_net_kw: false,
            has_var_kw: false,
            init_node: None,
        }
    }
}

#[derive(Default)]
struct NonAnsiPorts {
    active: bool,
    ordered: Vec<(String, NodeRef)>,
    entries: HashMap<String, NonAnsiPortEntry>,
}

pub(crate) struct Elab<'b, 't> {
    b: &'b Builder<'t>,
    defs: HashMap<String, Def>,
    binds: Vec<Bind>,
    defparams: Vec<Vec<(Vec<String>, Over)>>,
    envs: Vec<Env>,
    pub out: Vec<String>,
    pub diags: Vec<crate::names::Diag>,
    checked: HashSet<NodeRef>,
    /// `-top` names; empty: every module nothing instantiates.
    pub top: Vec<String>,
    /// CLI parameter overrides (`-G name=val` or `-G path.name=val`).
    pub cli_params: Vec<(String, String)>,
    taken: HashSet<NodeRef>,
    considered: HashSet<NodeRef>,
    reached: HashSet<NodeRef>,
    pub skipped: Vec<NodeRef>,
    pub copies: HashMap<NodeRef, (usize, usize)>,
    uninst: usize,
    bound: usize,
    recording: usize,
    rec: Vec<(NodeRef, usize)>,
    pub(crate) inst_log: Vec<InstLog>,
    inst_stack: Vec<usize>,
    /// Arena-backed elaborated design object model.
    pub design: ElaboratedDesign,
    locator: crate::Locator,
    design_stack: Vec<InstanceId>,
    gen_stack: Vec<String>,
    non_ansi_stack: Vec<NonAnsiPorts>,
    matched_cli_params: HashSet<usize>,
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
            let mut ends: Vec<(u32, NodeRef)> = Vec::new();
            for n in t.preorder() {
                if matches!(t.kind(n), ModuleDecl | InterfaceDecl | ProgramDecl) {
                    if let Some((_, hi)) = t.token_bounds(n) {
                        ends.push((hi, NodeRef { tree: ti, node: n }));
                    }
                }
                if t.kind(n) != BindDirective {
                    continue;
                }
                let at = t.token_bounds(n).map_or(0, |(lo, _)| lo);
                let container = ends
                    .iter()
                    .rev()
                    .find(|(hi, d)| t.token_bounds(d.node).is_some_and(|(lo, _)| lo <= at) && at <= *hi);
                let container = container.map(|c| c.1);
                let r = NodeRef { tree: ti, node: n };
                let kids = b.children(r);
                let Some(&target) = kids.first() else { continue };
                if b.kind(target) == DataType {
                    continue;
                }
                let target = b.toks_of(target).concat();
                let list = b.own_tokens(r).contains(&":").then(|| {
                    kids.iter()
                        .skip(1)
                        .take_while(|k| b.kind(**k) != DataType)
                        .map(|k| b.toks_of(*k).concat())
                        .collect()
                });
                binds.push((target, list, r, container));
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
            cli_params: Vec::new(),
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
            design: ElaboratedDesign::default(),
            locator: crate::Locator::default(),
            design_stack: Vec::new(),
            gen_stack: Vec::new(),
            non_ansi_stack: Vec::new(),
            matched_cli_params: HashSet::default(),
        }
    }

    fn loc_of(&mut self, r: NodeRef) -> crate::Loc {
        let t = &self.b.trees[r.tree];
        t.token_bounds(r.node).map_or_else(crate::Loc::default, |(tok, _)| self.locator.loc(r.tree, t, tok))
    }

    fn cur_gen_prefix(&self) -> String {
        self.gen_stack.last().cloned().unwrap_or_default()
    }

    #[allow(clippy::too_many_arguments)]
    fn add_symbol(
        &mut self,
        raw_name: &str,
        gen_prefix: &str,
        kind: SymbolKind,
        ty: RtlType,
        packed_bounds: Option<(i128, i128)>,
        decl: NodeRef,
        init_node: Option<NodeRef>,
        cval: Option<CVal>,
    ) -> Option<SymbolId> {
        if self.uninst > 0 {
            return None;
        }
        let &inst_id = self.design_stack.last()?;
        let name = raw_name.strip_prefix('\\').unwrap_or(raw_name).to_string();
        if name.is_empty() {
            return None;
        }
        let rel_path = if gen_prefix.is_empty() { name.clone() } else { format!("{gen_prefix}.{name}") };
        let inst_path = self.design.instances[inst_id.0 as usize].path.clone();
        let full_path = format!("{inst_path}.{rel_path}");
        let loc = self.loc_of(decl);
        let id = SymbolId(self.design.symbols.len() as u32);
        self.design.symbols.push(DesignSymbol {
            id,
            instance: inst_id,
            name,
            rel_path: rel_path.clone(),
            full_path: full_path.clone(),
            kind,
            ty,
            packed_bounds,
            decl_node: (decl.tree, decl.node),
            init_node: init_node.map(|n| (n.tree, n.node)),
            init_scope_prefix: gen_prefix.to_string(),
            cval,
            loc,
        });
        let inst = &mut self.design.instances[inst_id.0 as usize];
        inst.symbols.push(id);
        inst.sym_by_rel.insert(rel_path, id);
        self.design.sym_by_path.insert(full_path, id);
        Some(id)
    }

    fn push_item(&mut self, kind: ElabItemKind, r: NodeRef, vals: &Vals) {
        if self.uninst > 0 {
            return;
        }
        let Some(&inst_id) = self.design_stack.last() else { return };
        let scope_prefix = self.cur_gen_prefix();
        if matches!(kind, ElabItemKind::FunctionDecl | ElabItemKind::TaskDecl) {
            if let Some(n) = self.b.child(r, Name) {
                let toks = self.b.toks_of(n);
                if !toks.contains(&"::") {
                    if let Some(&last) = toks.last() {
                        let clean = last.strip_prefix('\\').unwrap_or(last);
                        self.design.instances[inst_id.0 as usize].subroutines.push((
                            scope_prefix.clone(),
                            clean.to_string(),
                            kind,
                            r.tree,
                            r.node,
                        ));
                    }
                }
            }
        }
        let loc = self.loc_of(r);
        let gen_consts = vals.active_consts();
        self.design.instances[inst_id.0 as usize].items.push(ElabItem {
            kind,
            tree: r.tree,
            node: r.node,
            scope_prefix,
            gen_consts,
            loc,
        });
    }

    fn record_typedef(&mut self, td: NodeRef, tname: &str, scope: usize, vals: &Vals) {
        if self.uninst > 0 {
            return;
        }
        let Some(&inst_id) = self.design_stack.last() else { return };
        let b = self.b;
        let Some(dt) = b.child(td, DataType) else { return };
        let env = self.env();
        *b.inst.borrow_mut() = vals.clone();
        let udims: Vec<NodeRef> = b.children(td).into_iter().filter(|c| b.kind(*c) == Dim).collect();
        let resolved = if udims.is_empty() {
            b.rtl_data_type(dt, scope, &env, Some(tname.to_string()), 0)
        } else {
            b.rtl_type(Some(dt), Some(td), scope, &env, 0)
        };
        *b.inst.borrow_mut() = Vals::default();
        let prefix = self.cur_gen_prefix();
        self.design.instances[inst_id.0 as usize].typedefs.push((prefix, tname.to_string(), resolved));
    }

    fn record_import(&mut self, imp: NodeRef) {
        if self.uninst > 0 {
            return;
        }
        let Some(&inst_id) = self.design_stack.last() else { return };
        let toks = self.b.toks_of(imp);
        let prefix = self.cur_gen_prefix();
        let mut i = 1;
        while i + 2 < toks.len() {
            if toks[i + 1] == "::" {
                let item = if toks[i + 2] == "*" { None } else { Some(toks[i + 2].to_string()) };
                self.design.instances[inst_id.0 as usize].imports.push((prefix.clone(), toks[i].to_string(), item));
                i += 3;
            } else {
                i += 1;
            }
            if i < toks.len() && toks[i] == "," {
                i += 1;
            }
        }
    }

    fn collect_packages_and_unit(&mut self) {
        let pkg_entries: Vec<(String, usize)> = self.b.packages.iter().map(|(k, &v)| (k.clone(), v)).collect();
        for (pkg_name, scope_idx) in pkg_entries {
            let pscope = self.build_package_scope(&pkg_name, scope_idx);
            self.design.packages.insert(pkg_name, pscope);
        }
        let unit_scope = self.build_package_scope("$unit", self.b.unit);
        self.design.unit_scope = unit_scope;
    }

    fn build_package_scope(&mut self, name: &str, scope_idx: usize) -> PackageScope {
        let b = self.b;
        let empty: Env = Rc::new(HashMap::default());
        let mut out = PackageScope {
            name: name.to_string(),
            imports: b.scopes[scope_idx].imports.clone(),
            params: Vec::new(),
            enum_consts: Vec::new(),
            typedefs: Vec::new(),
            subroutines: Vec::new(),
        };
        for (ti, t) in b.trees.iter().enumerate() {
            let root = NodeRef { tree: ti, node: t.root };
            for c in b.children(root) {
                match b.kind(c) {
                    PackageDecl if name != "$unit" => {
                        if b.name_of(c).is_some_and(|(n, _)| n == name) {
                            for item in b.children(c) {
                                self.populate_pkg_item(&mut out, item, scope_idx, &empty);
                            }
                        }
                    }
                    ParamDecl | TypedefDecl | DataDecl | FunctionDecl | TaskDecl | LetDecl | SequenceDecl
                    | PropertyDecl
                        if name == "$unit" =>
                    {
                        self.populate_pkg_item(&mut out, c, scope_idx, &empty);
                    }
                    _ => {}
                }
            }
        }
        out
    }

    fn populate_pkg_item(&mut self, out: &mut PackageScope, item: NodeRef, scope_idx: usize, empty: &Env) {
        let b = self.b;
        match b.kind(item) {
            ParamDecl if !b.own_tokens(item).contains(&"type") && !b.own_tokens(item).contains(&"defparam") => {
                let dt = b.child(item, DataType);
                let shape = dt.and_then(|t| b.int_shape(t, scope_idx, empty, 0));
                let two = dt.is_some_and(|t| b.two_state(t));
                for d in b.children(item).into_iter().filter(|d| b.kind(*d) == Declarator) {
                    let Some((pname, _)) = b.name_of(d) else { continue };
                    let Some(vnode) = b.children(d).into_iter().find(|k| !matches!(b.kind(*k), Name | Dim)) else {
                        continue;
                    };
                    let val = match dt {
                        Some(t) if b.kind(vnode) == AssignPattern => {
                            b.eval_typed(vnode, Ty::of(t, scope_idx), scope_idx, empty, 0)
                        }
                        _ => b.eval_xz(vnode, scope_idx, empty, two).map(|c| match shape {
                            Some((w, s)) if c.w < w => {
                                let x = b.eval_ctx(vnode, w, s, scope_idx, empty, 0).unwrap_or_else(|| c.cast(w, s));
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
                    if let Some(cv) = val {
                        let pty = match dt {
                            Some(_) => b.rtl_type(dt, Some(d), scope_idx, empty, 0),
                            None => RtlType::Integral { width: cv.w.max(1), signed: cv.s, four_state: !cv.known() },
                        };
                        let loc = self.loc_of(d);
                        out.params.push((pname, cv, pty, loc));
                    }
                }
            }
            TypedefDecl => {
                let alias = b.name_of(item).map(|x| x.0);
                if let Some(dt) = b.child(item, DataType) {
                    let rty = b.rtl_type(Some(dt), Some(item), scope_idx, empty, 0);
                    if let RtlType::Enum { variants, .. } = &rty {
                        for (vn, vc) in variants {
                            out.enum_consts.push((vn.clone(), vc.clone(), rty.clone()));
                        }
                    }
                    if let Some(tname) = alias {
                        out.typedefs.push((tname, rty));
                    }
                }
            }
            DataDecl => {
                if let Some(dt) = b.child(item, DataType) {
                    if b.own_tokens(dt).first() == Some(&"enum") {
                        let rty = b.rtl_data_type(dt, scope_idx, empty, None, 0);
                        if let RtlType::Enum { variants, .. } = &rty {
                            for (vn, vc) in variants {
                                out.enum_consts.push((vn.clone(), vc.clone(), rty.clone()));
                            }
                        }
                    }
                }
            }
            FunctionDecl | TaskDecl | LetDecl | SequenceDecl | PropertyDecl => {
                if let Some(n) = b.child(item, Name) {
                    let toks = b.toks_of(n);
                    if !toks.contains(&"::") {
                        if let Some(&last) = toks.last() {
                            let clean = last.strip_prefix('\\').unwrap_or(last);
                            let kind = match b.kind(item) {
                                FunctionDecl => ElabItemKind::FunctionDecl,
                                TaskDecl => ElabItemKind::TaskDecl,
                                LetDecl => ElabItemKind::LetDecl,
                                SequenceDecl => ElabItemKind::SequenceDecl,
                                _ => ElabItemKind::PropertyDecl,
                            };
                            let loc = self.loc_of(item);
                            out.subroutines.push((clean.to_string(), kind, item.tree, item.node, loc));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn record_enums_in(&mut self, n: NodeRef, scope: usize, vals: &Vals, alias: Option<String>) {
        if self.uninst > 0 {
            return;
        }
        let Some(&inst_id) = self.design_stack.last() else { return };
        let b = self.b;
        let env = self.env();
        *b.inst.borrow_mut() = vals.clone();
        let mut stack: Vec<(NodeRef, Option<String>)> =
            b.children(n).into_iter().filter(|c| b.kind(*c) == DataType).map(|c| (c, alias.clone())).collect();
        for c in b.children(n).into_iter().filter(|c| b.kind(*c) == Declarator) {
            stack.extend(b.children(c).into_iter().filter(|d| b.kind(*d) == DataType).map(|d| (d, None)));
        }
        let prefix = self.cur_gen_prefix();
        while let Some((dt, al)) = stack.pop() {
            match b.own_tokens(dt).first().copied() {
                Some("enum") => {
                    let ety = b.rtl_data_type(dt, scope, &env, al, 0);
                    if let RtlType::Enum { variants, .. } = &ety {
                        for (vname, cval) in variants {
                            self.design.instances[inst_id.0 as usize].enum_consts.push((
                                prefix.clone(),
                                vname.clone(),
                                cval.clone(),
                                ety.clone(),
                            ));
                        }
                    }
                }
                Some("struct" | "union") => {}
                _ => {
                    stack.extend(b.children(dt).into_iter().filter(|c| b.kind(*c) == DataType).map(|c| (c, None)));
                }
            }
        }
        *b.inst.borrow_mut() = Vals::default();
    }

    fn record_modport(&mut self, n: NodeRef) {
        if self.uninst > 0 {
            return;
        }
        let Some(&inst_id) = self.design_stack.last() else { return };
        let b = self.b;
        let t = b.tree(n);
        for d in b.children(n).into_iter().filter(|c| b.kind(*c) == Declarator) {
            let Some((mp_name, _)) = b.name_of(d) else { continue };
            let mut ports = Vec::new();
            let mut dir = PortDir::Inout;
            let mut in_parens = false;
            let mut skip_depth = 0usize;
            for &ti in &t.token_range(d.node) {
                let txt = &*t.tokens[ti as usize].text;
                if txt == "(" {
                    if in_parens {
                        skip_depth += 1;
                    } else {
                        in_parens = true;
                    }
                    continue;
                }
                if txt == ")" {
                    if skip_depth > 0 {
                        skip_depth -= 1;
                    } else {
                        in_parens = false;
                    }
                    continue;
                }
                if !in_parens || skip_depth > 0 {
                    continue;
                }
                match txt {
                    "input" => dir = PortDir::Input,
                    "output" => dir = PortDir::Output,
                    "inout" => dir = PortDir::Inout,
                    "ref" => dir = PortDir::Ref,
                    "," | "." | "clocking" | "import" | "export" | "function" | "task" => {}
                    ident => {
                        let clean = ident.strip_prefix('\\').unwrap_or(ident);
                        if !clean.is_empty()
                            && clean.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                        {
                            ports.push((clean.to_string(), dir));
                        }
                    }
                }
            }
            self.design.instances[inst_id.0 as usize].modports.push((mp_name, ports));
        }
    }

    fn eval_init(&self, init: NodeRef, dt: Option<NodeRef>, ty: &RtlType, scope: usize, vals: &Vals) -> Option<CVal> {
        let b = self.b;
        let env = self.env();
        *b.inst.borrow_mut() = vals.clone();
        let res = match dt {
            Some(d) if b.kind(init) == AssignPattern => b.eval_typed(init, Ty::of(d, scope), scope, &env, 0),
            _ => {
                let two = !ty.is_four_state();
                b.eval_xz(init, scope, &env, two).map(|c| match ty.integral_shape() {
                    Some((w, s, _)) if c.w < w => {
                        let x = b.eval_ctx(init, w, s, scope, &env, 0).unwrap_or_else(|| c.cast(w, s));
                        if two {
                            x.two_state()
                        } else {
                            x
                        }
                    }
                    Some((w, s, _)) => c.cast(w, s),
                    None => c,
                })
            }
        };
        *b.inst.borrow_mut() = Vals::default();
        res
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
                    && !self.needs_params_for_top(n, d.decl)
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

    fn has_cli_override(&self, mod_name: &str, param_name: &str) -> bool {
        let scoped = format!("{mod_name}.{param_name}");
        self.cli_params.iter().any(|(k, _)| k == param_name || k == &scoped)
    }

    /// A header parameter without a default (unless supplied via `-G`).
    fn needs_params_for_top(&self, mod_name: &str, decl: NodeRef) -> bool {
        let b = self.b;
        let Some(pl) = b.child(decl, Header).and_then(|h| b.child(h, ParamList)) else { return false };
        b.children(pl).into_iter().filter(|p| b.kind(*p) == ParamDecl).any(|p| {
            b.children(p).into_iter().filter(|d| b.kind(*d) == Declarator).any(|d| {
                let no_default = b.children(d).into_iter().all(|c| matches!(b.kind(c), Name | Dim));
                if !no_default {
                    return false;
                }
                match b.name_of(d) {
                    Some((pname, _)) => !self.has_cli_override(mod_name, &pname),
                    None => true,
                }
            })
        })
    }

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
        loop {
            self.run_once();
            let before = self.binds.len();
            let reached = &self.reached;
            self.binds.retain(|bd| bd.3.is_none_or(|c| reached.contains(&c)));
            if self.binds.len() == before {
                return;
            }
            self.reset();
        }
    }

    fn reset(&mut self) {
        self.defparams.clear();
        self.envs = vec![Rc::new(HashMap::default())];
        self.out.clear();
        self.diags.clear();
        self.checked.clear();
        self.taken.clear();
        self.considered.clear();
        self.reached.clear();
        self.skipped.clear();
        self.copies.clear();
        self.uninst = 0;
        self.bound = 0;
        self.recording = 0;
        self.rec.clear();
        self.inst_log.clear();
        self.inst_stack.clear();
        self.design = ElaboratedDesign::default();
        self.locator = crate::Locator::default();
        self.design_stack.clear();
        self.gen_stack.clear();
        self.non_ansi_stack.clear();
        self.matched_cli_params.clear();
    }

    fn run_once(&mut self) {
        self.collect_packages_and_unit();
        let tops = self.tops();
        for top in &tops {
            self.interface_ports(top);
            self.out.push(format!("$root instance {top} : {top}"));
            let decl = self.defs.get(top).map(|d| d.decl).unwrap_or(NodeRef { tree: 0, node: NodeId(0) });
            let (over, pend) = self.cli_overrides_for_top(top, decl);
            let loc = self.loc_of(decl);
            self.instance(top, top, top, &over, pend, Rc::new(HashMap::default()), Vec::new(), loc, 0);
        }
        // Check that every `-G` parameter override matched at least one elaborated parameter.
        let diag_anchor = tops
            .first()
            .and_then(|t| self.defs.get(t).map(|d| d.decl))
            .or_else(|| (!self.b.trees.is_empty()).then(|| NodeRef { tree: 0, node: self.b.trees[0].root }));
        if let Some(anchor) = diag_anchor {
            for (idx, (k, _)) in self.cli_params.iter().enumerate() {
                if !self.matched_cli_params.contains(&idx) {
                    self.diags.push(crate::names::Diag {
                        tree: anchor.tree,
                        node: anchor.node,
                        message: format!("unknown parameter override '{k}'"),
                    });
                }
            }
        }
        self.design.files = self.locator.files().to_vec();
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

    fn cli_overrides_for_top(&mut self, top: &str, decl: NodeRef) -> (Overrides, Vec<(Vec<String>, Over)>) {
        let mut over = Overrides::default();
        let mut pend = Vec::new();
        let order = self.param_order(decl);
        let insert_cli_over = |over: &mut Overrides, key: String, mut next: Over| {
            if let Some(prev) = over.remove(&key) {
                next.cli_indices.extend(prev.cli_indices);
            }
            over.insert(key, next);
        };
        for (idx, (key, raw)) in self.cli_params.iter().enumerate() {
            let mk_over = |raw: &str, idx: usize| Over {
                val: eval_cli_expr(raw, None, false).map(|x| x.0),
                node: decl,
                scope: self.b.unit,
                vals: Vals::default(),
                env: Rc::new(HashMap::default()),
                cli_raw: Some(raw.to_string()),
                cli_indices: vec![idx],
            };
            if !key.contains('.') {
                if order.iter().any(|(n, is_ty)| !*is_ty && n == key) {
                    insert_cli_over(&mut over, key.clone(), mk_over(raw, idx));
                }
                continue;
            }
            let segs: Vec<String> = key.split('.').map(str::to_string).collect();
            if segs.first().map(String::as_str) == Some(top) {
                if segs.len() == 2 {
                    if order.iter().any(|(n, is_ty)| !*is_ty && n == &segs[1]) {
                        insert_cli_over(&mut over, segs[1].clone(), mk_over(raw, idx));
                    }
                } else if segs.len() > 2 {
                    pend.push((segs[1..].to_vec(), mk_over(raw, idx)));
                }
            } else {
                pend.push((segs, mk_over(raw, idx)));
            }
        }
        (over, pend)
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
    #[allow(clippy::too_many_arguments)]
    fn instance(
        &mut self,
        def: &str,
        leaf_name: &str,
        path: &str,
        over: &Overrides,
        pend: Vec<(Vec<String>, Over)>,
        env: Env,
        port_conns: Vec<PortConn>,
        loc: crate::Loc,
        depth: usize,
    ) {
        if depth > MAX_DEPTH {
            return;
        }
        self.defparams.push(pend);
        self.envs.push(env);
        self.instance_body(def, leaf_name, path, over, port_conns, loc, depth);
        self.envs.pop();
        self.defparams.pop();
    }

    fn env(&self) -> Env {
        self.envs.last().cloned().unwrap_or_default()
    }

    #[allow(clippy::too_many_arguments)]
    fn instance_body(
        &mut self,
        def: &str,
        leaf_name: &str,
        path: &str,
        over: &Overrides,
        port_conns: Vec<PortConn>,
        loc: crate::Loc,
        depth: usize,
    ) {
        let Some(d) = self.defs.get(def) else { return };
        let (decl, scope) = (d.decl, d.scope);
        self.reached.insert(decl);
        let logged = self.bound == 0;
        if logged {
            self.inst_log.push(InstLog { decl, vals: Vals::default(), env: self.env(), copies: HashMap::default() });
            self.inst_stack.push(self.inst_log.len() - 1);
        }
        let pushed_design = self.uninst == 0;
        if pushed_design {
            let parent = self.design_stack.last().copied();
            let rel_path = match parent {
                Some(pid) => {
                    let ppath = &self.design.instances[pid.0 as usize].path;
                    path.strip_prefix(&format!("{ppath}.")).unwrap_or(leaf_name).to_string()
                }
                None => leaf_name.to_string(),
            };
            let inst_id = InstanceId(self.design.instances.len() as u32);
            let is_interface = self.b.kind(decl) == InterfaceDecl;
            self.design.instances.push(DesignInstance {
                id: inst_id,
                parent,
                name: leaf_name.to_string(),
                rel_path,
                path: path.to_string(),
                definition: def.to_string(),
                is_interface,
                is_bound: self.bound > 0,
                def_node: (decl.tree, decl.node),
                params: Vec::new(),
                type_params: Vec::new(),
                typedefs: Vec::new(),
                imports: Vec::new(),
                ports: Vec::new(),
                port_conns,
                symbols: Vec::new(),
                enum_consts: Vec::new(),
                modports: Vec::new(),
                items: Vec::new(),
                subroutines: Vec::new(),
                children: Vec::new(),
                loc,
                sym_by_rel: HashMap::default(),
            });
            self.design.inst_by_path.insert(path.to_string(), inst_id);
            match parent {
                Some(pid) => self.design.instances[pid.0 as usize].children.push(inst_id),
                None => self.design.tops.push(inst_id),
            }
            self.design_stack.push(inst_id);
            self.gen_stack.push(String::new());
            self.non_ansi_stack.push(NonAnsiPorts::default());
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
                    self.record_type_params(p, scope, &vals);
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
                    self.record_type_params(p, scope, &vals);
                    continue;
                }
                self.params(p, dt, !local, path, scope, over, &mut vals);
            }
        }
        if pushed_design {
            if let Some(h) = b.child(decl, Header) {
                for imp in b.children(h) {
                    if b.kind(imp) == ImportDecl {
                        self.record_import(imp);
                    }
                }
            }
            self.collect_header_ports(decl, scope, &vals);
        }
        let body: Vec<NodeRef> = b.children(decl);
        self.defparams_of(&body, scope, over, header.is_none(), &vals);
        self.items(&body, path, scope, over, header.is_none(), &mut vals, depth);
        if pushed_design {
            self.finalize_non_ansi_ports(scope, &vals);
        }
        self.check_selects(decl, scope, &vals);
        if logged {
            if let Some(i) = self.inst_stack.pop() {
                self.inst_log[i].vals = vals.clone();
            }
        }
        // Instances bound into this one: keep `design_stack` active so bound children attach under `inst_id`!
        let binds: Vec<NodeRef> = self
            .binds
            .iter()
            .filter(|(target, list, _, _)| match list {
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
        if pushed_design {
            self.non_ansi_stack.pop();
            self.gen_stack.pop();
            self.design_stack.pop();
        }
    }

    fn record_type_params(&mut self, p: NodeRef, scope: usize, vals: &Vals) {
        if self.uninst > 0 {
            return;
        }
        let Some(&inst_id) = self.design_stack.last() else { return };
        let b = self.b;
        let env = self.env();
        *b.inst.borrow_mut() = vals.clone();
        for d in b.children(p) {
            if b.kind(d) != Declarator {
                continue;
            }
            let Some((name, _)) = b.name_of(d) else { continue };
            let rty = if let Some(bd) = env.get(&name) {
                b.rtl_data_type(bd.node, bd.scope, &bd.env, Some(name.clone()), 0)
            } else if let Some(def_dt) = b.child(d, DataType).or_else(|| b.child(p, DataType)) {
                b.rtl_data_type(def_dt, scope, &env, Some(name.clone()), 0)
            } else {
                RtlType::Integral { width: 1, signed: false, four_state: true }
            };
            self.design.instances[inst_id.0 as usize].type_params.push((name, rty));
        }
        *b.inst.borrow_mut() = Vals::default();
    }

    fn collect_header_ports(&mut self, decl: NodeRef, scope: usize, vals: &Vals) {
        let b = self.b;
        let Some(pl) = b.child(decl, Header).and_then(|h| b.child(h, PortList)) else { return };
        let ports: Vec<NodeRef> = b.children(pl).into_iter().filter(|p| b.kind(*p) == Port).collect();
        if ports.is_empty() {
            return;
        }
        let is_ansi = {
            let p0 = ports[0];
            let kids = b.children(p0);
            let own = b.own_tokens(p0);
            let bare = own.is_empty()
                && kids.len() == 1
                && b.kind(kids[0]) == Declarator
                && b.children(kids[0]).len() == 1
                && b.child(kids[0], Name).is_some();
            let lead = b.toks_of(p0).first().copied();
            !(bare || lead == Some(".") || lead == Some("{"))
        };
        if !is_ansi {
            if let Some(na) = self.non_ansi_stack.last_mut() {
                na.active = true;
                for p in ports {
                    let own = b.own_tokens(p);
                    if own.first() == Some(&".") {
                        if let Some((pname, _)) = b.name_of(p) {
                            na.ordered.push((pname, p));
                        }
                    } else if let Some(d) = b.child(p, Declarator) {
                        if let Some((pname, _)) = b.name_of(d) {
                            na.ordered.push((pname, d));
                        }
                    }
                }
            }
            return;
        }
        let env = self.env();
        let mut dir = PortDir::Inout;
        let mut cur_dt: Option<NodeRef> = None;
        let mut explicit_net = false;
        let mut explicit_var = false;
        for p in ports {
            self.record_enums_in(p, scope, vals, None);
            let own = b.own_tokens(p);
            let dt = b.child(p, DataType);
            let Some(d) = b.child(p, Declarator) else { continue };
            let Some((pname, _)) = b.name_of(d) else { continue };
            // Interface port (`interface p`, `bus.master p`, or `bus p`).
            let is_iface_name = |nm: &str| self.defs.get(nm).is_some_and(|x| b.kind(x.decl) == InterfaceDecl);
            if own.first() == Some(&"interface") {
                let mp = (own.get(1) == Some(&".")).then(|| own.get(2).map(|s| s.to_string())).flatten();
                let ty = RtlType::Interface { def: "interface".into(), modport: mp };
                if let Some(sid) = self.add_symbol(
                    &pname,
                    "",
                    SymbolKind::Port { dir: PortDir::Inout, is_net: false },
                    ty,
                    None,
                    d,
                    None,
                    None,
                ) {
                    if let Some(&inst_id) = self.design_stack.last() {
                        self.design.instances[inst_id.0 as usize].ports.push(sid);
                    }
                }
                continue;
            }
            if dt.is_none() && own.get(1) == Some(&".") && own.first().is_some_and(|n| is_iface_name(n)) {
                let ty = RtlType::Interface { def: own[0].to_string(), modport: own.get(2).map(|s| s.to_string()) };
                if let Some(sid) = self.add_symbol(
                    &pname,
                    "",
                    SymbolKind::Port { dir: PortDir::Inout, is_net: false },
                    ty,
                    None,
                    d,
                    None,
                    None,
                ) {
                    if let Some(&inst_id) = self.design_stack.last() {
                        self.design.instances[inst_id.0 as usize].ports.push(sid);
                    }
                }
                continue;
            }
            if !own.iter().any(|x| DIR_KEYWORDS.contains(x)) {
                if let Some(dnode) = dt {
                    let dtoks = b.toks_of(dnode);
                    if dtoks.len() == 1 && is_iface_name(dtoks[0]) {
                        let ty = RtlType::Interface { def: dtoks[0].to_string(), modport: None };
                        if let Some(sid) = self.add_symbol(
                            &pname,
                            "",
                            SymbolKind::Port { dir: PortDir::Inout, is_net: false },
                            ty,
                            None,
                            d,
                            None,
                            None,
                        ) {
                            if let Some(&inst_id) = self.design_stack.last() {
                                self.design.instances[inst_id.0 as usize].ports.push(sid);
                            }
                        }
                        continue;
                    }
                }
            }
            let new_dir = own.iter().find_map(|&t| parse_port_dir(t));
            let has_net = own.iter().any(|t| NET_KEYWORDS.contains(t));
            let has_var = own.contains(&"var");
            if new_dir.is_some() || dt.is_some() || has_net || has_var {
                if let Some(nd) = new_dir {
                    dir = nd;
                }
                cur_dt = dt;
                explicit_net = has_net;
                explicit_var = has_var;
            }
            *b.inst.borrow_mut() = vals.clone();
            let ty = b.rtl_type(cur_dt, Some(d), scope, &env, 0);
            let pbounds = b.packed_1d_bounds(cur_dt, scope, &env, 0);
            *b.inst.borrow_mut() = Vals::default();
            let explicit_dt = cur_dt
                .and_then(|dn| b.toks_of(dn).first().copied())
                .is_some_and(|f| !matches!(f, "[" | "signed" | "unsigned"));
            let is_net = explicit_net
                || !(explicit_var
                    || dir == PortDir::Ref
                    || !ty.is_packed()
                    || !ty.is_four_state()
                    || matches!(ty, RtlType::PackedStruct { .. } | RtlType::PackedUnion { .. } | RtlType::Enum { .. })
                    || (dir == PortDir::Output && explicit_dt));
            let init = b.children(d).into_iter().find(|c| !matches!(b.kind(*c), Name | Dim));
            let cval = init.and_then(|iv| self.eval_init(iv, cur_dt, &ty, scope, vals));
            if let Some(sid) = self.add_symbol(&pname, "", SymbolKind::Port { dir, is_net }, ty, pbounds, d, init, cval)
            {
                if let Some(&inst_id) = self.design_stack.last() {
                    self.design.instances[inst_id.0 as usize].ports.push(sid);
                }
            }
        }
    }

    fn finalize_non_ansi_ports(&mut self, scope: usize, vals: &Vals) {
        let Some(na) = self.non_ansi_stack.last() else { return };
        if !na.active {
            return;
        }
        let ordered = na.ordered.clone();
        let b = self.b;
        let env = self.env();
        for (pname, port_node) in ordered {
            let entry = self.non_ansi_stack.last().and_then(|s| s.entries.get(&pname));
            let dir = entry.map_or(PortDir::Inout, |e| e.dir);
            let dt = entry.and_then(|e| e.decl_dt.or(e.dir_dt));
            let decl_item = entry.and_then(|e| e.decl_item).unwrap_or(port_node);
            let has_net_kw = entry.is_some_and(|e| e.has_net_kw);
            let has_var_kw = entry.is_some_and(|e| e.has_var_kw);
            let init = entry.and_then(|e| e.init_node);
            *b.inst.borrow_mut() = vals.clone();
            let ty = b.rtl_type(dt, Some(decl_item), scope, &env, 0);
            let pbounds = b.packed_1d_bounds(dt, scope, &env, 0);
            *b.inst.borrow_mut() = Vals::default();
            let explicit_dt = dt
                .and_then(|dn| b.toks_of(dn).first().copied())
                .is_some_and(|f| !matches!(f, "[" | "signed" | "unsigned"));
            let is_net = has_net_kw
                || !(has_var_kw || dir == PortDir::Ref || !ty.is_packed() || !ty.is_four_state() || explicit_dt);
            let cval = init.and_then(|iv| self.eval_init(iv, dt, &ty, scope, vals));
            if let Some(sid) =
                self.add_symbol(&pname, "", SymbolKind::Port { dir, is_net }, ty, pbounds, decl_item, init, cval)
            {
                if let Some(&inst_id) = self.design_stack.last() {
                    self.design.instances[inst_id.0 as usize].ports.push(sid);
                }
            }
        }
    }

    fn record_data_decl(&mut self, c: NodeRef, scope: usize, vals: &Vals) {
        if self.uninst > 0 {
            return;
        }
        self.record_enums_in(c, scope, vals, None);
        let b = self.b;
        let own = b.own_tokens(c);
        let dt = b.child(c, DataType);
        let gen_prefix = self.cur_gen_prefix();
        let dir_kw = own.first().copied().and_then(parse_port_dir);
        let has_net_kw = own.iter().any(|t| NET_KEYWORDS.contains(t));
        let has_var_kw = own.iter().any(|&t| t == "var" || t == "reg");

        // At module root, merge directional `input`/`output` declarations and matching non-ANSI port
        // redeclarations (`reg [7:0] q;`) into `NonAnsiPorts`.
        if gen_prefix.is_empty() {
            if let Some(na) = self.non_ansi_stack.last_mut().filter(|na| na.active) {
                let mut all_consumed = true;
                for d in b.children(c).into_iter().filter(|d| b.kind(*d) == Declarator) {
                    let Some((name, _)) = b.name_of(d) else { continue };
                    let is_port = dir_kw.is_some() || na.ordered.iter().any(|(pn, _)| pn == &name);
                    if is_port {
                        let e = na
                            .entries
                            .entry(name)
                            .or_insert_with(|| NonAnsiPortEntry { dir: PortDir::Inout, ..NonAnsiPortEntry::default() });
                        if let Some(dk) = dir_kw {
                            e.dir = dk;
                            if dt.is_some() {
                                e.dir_dt = dt;
                            }
                        } else if dt.is_some() {
                            e.decl_dt = dt;
                        }
                        e.decl_item = Some(d);
                        e.has_net_kw |= has_net_kw;
                        e.has_var_kw |= has_var_kw;
                        if let Some(iv) = b.children(d).into_iter().find(|k| !matches!(b.kind(*k), Name | Dim)) {
                            e.init_node = Some(iv);
                        }
                    } else {
                        all_consumed = false;
                    }
                }
                if all_consumed {
                    return;
                }
            }
        }

        let env = self.env();
        let net_kw = own.iter().find(|t| NET_KEYWORDS.contains(t)).copied();
        for d in b.children(c).into_iter().filter(|d| b.kind(*d) == Declarator) {
            let Some((name, _)) = b.name_of(d) else { continue };
            if gen_prefix.is_empty()
                && self
                    .non_ansi_stack
                    .last()
                    .is_some_and(|na| na.active && na.ordered.iter().any(|(pn, _)| pn == &name))
            {
                continue;
            }
            *b.inst.borrow_mut() = vals.clone();
            let ty = b.rtl_type(dt, Some(d), scope, &env, 0);
            let pbounds = b.packed_1d_bounds(dt, scope, &env, 0);
            *b.inst.borrow_mut() = Vals::default();
            let init = b.children(d).into_iter().find(|k| !matches!(b.kind(*k), Name | Dim));
            let cval = init.and_then(|iv| self.eval_init(iv, dt, &ty, scope, vals));
            let kind = match net_kw {
                Some(nk) => SymbolKind::Net { net_type: nk.to_string() },
                None => SymbolKind::Var,
            };
            self.add_symbol(&name, &gen_prefix, kind, ty, pbounds, d, init, cval);
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
        self.record_enums_in(p, scope, vals, None);
        let b = self.b;
        let empty = self.env();
        let gen_prefix = self.cur_gen_prefix();
        for d in b.children(p) {
            if b.kind(d) != Declarator {
                continue;
            }
            let Some((name, _)) = b.name_of(d) else { continue };
            *b.inst.borrow_mut() = vals.clone();
            let shape = dt.and_then(|t| b.int_shape(t, scope, &empty, 0));
            let two_state_dt = dt.is_some_and(|t| b.two_state(t));
            let (text, val) = match over.get(&name).filter(|_| overridable) {
                Some(o) if o.cli_raw.is_some() => {
                    if self.uninst == 0 {
                        for &ci in &o.cli_indices {
                            self.matched_cli_params.insert(ci);
                        }
                    }
                    let raw = o.cli_raw.as_deref().unwrap_or_default();
                    match eval_cli_expr(raw, shape, two_state_dt) {
                        Some((cv, txt)) => (if self.uninst == 0 { Some(txt) } else { None }, Some(cv)),
                        None => {
                            if self.uninst == 0 {
                                self.diags.push(crate::names::Diag {
                                    tree: d.tree,
                                    node: d.node,
                                    message: format!("invalid parameter override for '{name}': '{raw}'"),
                                });
                            }
                            (None, None)
                        }
                    }
                }
                Some(o) => match o.val.clone().filter(|_| shape.is_some() || dt.is_none()) {
                    Some(v) => {
                        let v = match shape {
                            Some((w, s)) => {
                                *b.inst.borrow_mut() = o.vals.clone();
                                let c = b.eval_ctx(o.node, w, s, o.scope, &o.env, 0);
                                *b.inst.borrow_mut() = vals.clone();
                                c.unwrap_or_else(|| v.cast(w, s))
                            }
                            None => v,
                        };
                        (Some(v.show()), Some(v))
                    }
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
                            let text = if self.uninst == 0 { b.value_text(dt, v, scope, &empty) } else { None };
                            let two = two_state_dt;
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
                if self.uninst == 0 && !path.is_empty() {
                    self.out.push(format!("{path} parameter {name} = {t}"));
                }
            }
            if self.uninst == 0 && !path.is_empty() {
                *b.inst.borrow_mut() = vals.clone();
                let pty = match dt {
                    Some(_) => b.rtl_type(dt, Some(d), scope, &empty, 0),
                    None => match &val {
                        Some(cv) => RtlType::Integral { width: cv.w.max(1), signed: cv.s, four_state: !cv.known() },
                        None => RtlType::Integral { width: 32, signed: true, four_state: false },
                    },
                };
                let pbounds = b.packed_1d_bounds(dt, scope, &empty, 0);
                *b.inst.borrow_mut() = Vals::default();
                let init = b.children(d).into_iter().find(|c| !matches!(b.kind(*c), Name | Dim));
                self.add_symbol(
                    &name,
                    &gen_prefix,
                    SymbolKind::Param { local: !overridable },
                    pty,
                    pbounds,
                    d,
                    init,
                    val.clone(),
                );
                if gen_prefix.is_empty() {
                    if let Some(&inst_id) = self.design_stack.last() {
                        self.design.instances[inst_id.0 as usize].params.push((name.clone(), val.clone()));
                    }
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
                        self.record_type_params(c, scope, vals);
                        continue;
                    }
                    let overridable = body_params && own.contains(&"parameter");
                    self.params(c, b.child(c, DataType), overridable, path, scope, over, vals);
                }
                TypedefDecl => {
                    let alias = b.name_of(c).map(|x| x.0);
                    self.record_enums_in(c, scope, vals, alias.clone());
                    if let Some(tname) = alias {
                        self.record_typedef(c, &tname, scope, vals);
                    }
                }
                ImportDecl => self.record_import(c),
                DataDecl => self.record_data_decl(c, scope, vals),
                ModportDecl => {
                    self.record_modport(c);
                    self.push_item(ElabItemKind::ModportDecl, c, vals);
                }
                ProceduralBlock => self.push_item(ElabItemKind::ProceduralBlock, c, vals),
                ContinuousAssign => self.push_item(ElabItemKind::ContinuousAssign, c, vals),
                AssertStmt => self.push_item(ElabItemKind::AssertStmt, c, vals),
                ConcurrentAssert => self.push_item(ElabItemKind::ConcurrentAssert, c, vals),
                ClockingDecl => self.push_item(ElabItemKind::ClockingDecl, c, vals),
                DefaultDisable => self.push_item(ElabItemKind::DefaultDisable, c, vals),
                FunctionDecl => self.push_item(ElabItemKind::FunctionDecl, c, vals),
                TaskDecl => self.push_item(ElabItemKind::TaskDecl, c, vals),
                SequenceDecl => self.push_item(ElabItemKind::SequenceDecl, c, vals),
                PropertyDecl => self.push_item(ElabItemKind::PropertyDecl, c, vals),
                LetDecl => self.push_item(ElabItemKind::LetDecl, c, vals),
                GateInst => self.push_item(ElabItemKind::GateInst, c, vals),
                NetAlias => self.push_item(ElabItemKind::NetAlias, c, vals),
                GenerateRegion => self.walk(&b.children(c), path, scope, over, body_params, vals, depth, construct),
                Instantiation => self.instantiation(c, path, scope, vals, depth),
                LoopGenerate => {
                    *construct += 1;
                    self.loop_gen(c, path, scope, vals, depth, *construct);
                }
                GenerateBlock => {
                    *construct += 1;
                    self.block(c, None, None, path, scope, vals, depth, *construct);
                }
                IfGenerate | CaseGenerate => {
                    *construct += 1;
                    let n = *construct;
                    if self.uninst > 0 {
                        let mut all = Vec::new();
                        self.branches(c, &mut all);
                        for item in all {
                            self.block(item, None, None, path, scope, vals, depth, n);
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
                        self.block(item, None, None, path, scope, vals, depth, n);
                    }
                    let mut rest = Vec::new();
                    for &k in &chain {
                        self.branches(k, &mut rest);
                    }
                    self.uninst += 1;
                    for r in rest {
                        if Some(r) != item && !chain.contains(&r) {
                            self.block(r, None, None, path, scope, vals, depth, n);
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
            return if b.kind(item) == IfGenerate { self.choose(item, scope, vals) } else { Some(item) };
        }
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
        genvar: Option<(&str, &CVal, NodeRef)>,
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
        let cur_gen = self.cur_gen_prefix();
        let next_gen = if cur_gen.is_empty() { name } else { format!("{cur_gen}.{name}") };
        self.gen_stack.push(next_gen.clone());
        if let Some((gv_name, gv_val, gv_node)) = genvar {
            self.add_symbol(
                gv_name,
                &next_gen,
                SymbolKind::Genvar,
                RtlType::Integral { width: 32, signed: true, four_state: false },
                None,
                gv_node,
                None,
                Some(gv_val.clone()),
            );
        }
        let mut v = vals.child();
        self.items(&items, &inner, scope, &Overrides::default(), false, &mut v, depth);
        self.gen_stack.pop();
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
        let mut var_node = c;
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
                        var_node = r;
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
                self.block(body, None, None, path, scope, &v, depth, construct);
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
                c if k == 0 && (c.is_some() || self.uninst > 0) => {
                    *self.b.inst.borrow_mut() = Vals::default();
                    v.remove(&var);
                    self.uninst += 1;
                    self.block(body, None, None, path, scope, &v, depth, construct);
                    self.uninst -= 1;
                    break;
                }
                _ => break,
            }
            let before = self.out.len();
            if same.is_some() {
                skipped += 1;
            } else if self.uninst > 0 {
                let start = self.rec.len();
                let h0 = v.0.hits.get();
                self.recording += 1;
                self.block(body, Some(i.i()), None, path, scope, &v, depth, construct);
                self.recording -= 1;
                if v.0.hits.get() == h0 {
                    same = Some(self.rec[start..].to_vec());
                }
                if self.recording == 0 {
                    self.rec.truncate(start);
                }
            } else {
                self.block(body, Some(i.i()), Some((&var, &i, var_node)), path, scope, &v, depth, construct);
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

    fn extract_port_conns(&mut self, inst_node: NodeRef, vals: &Vals) -> Vec<PortConn> {
        let b = self.b;
        let Some(al) = b.child(inst_node, ArgList) else { return Vec::new() };
        let scope_prefix = self.cur_gen_prefix();
        let gen_consts = vals.active_consts();
        let mut out = Vec::new();
        for (idx, a) in b.children(al).into_iter().filter(|a| b.kind(*a) == Arg).enumerate() {
            let own = b.own_tokens(a);
            let expr = b.children(a).into_iter().find(|x| b.kind(*x) != Attribute).map(|x| (x.tree, x.node));
            let loc = self.loc_of(a);
            if own.starts_with(&[".", "*"]) {
                out.push(PortConn {
                    port_name: None,
                    arg_index: idx,
                    expr: None,
                    implicit_named: false,
                    wildcard: true,
                    scope_prefix: scope_prefix.clone(),
                    gen_consts: gen_consts.clone(),
                    loc,
                });
            } else if own.first() == Some(&".") {
                let pname = own.get(1).map(|s| s.strip_prefix('\\').unwrap_or(s).to_string());
                let has_parens = own.contains(&"(");
                out.push(PortConn {
                    port_name: pname,
                    arg_index: idx,
                    expr,
                    implicit_named: !has_parens && expr.is_none(),
                    wildcard: false,
                    scope_prefix: scope_prefix.clone(),
                    gen_consts: gen_consts.clone(),
                    loc,
                });
            } else {
                out.push(PortConn {
                    port_name: None,
                    arg_index: idx,
                    expr,
                    implicit_named: false,
                    wildcard: false,
                    scope_prefix: scope_prefix.clone(),
                    gen_consts: gen_consts.clone(),
                    loc,
                });
            }
        }
        out
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
                over.insert(
                    name,
                    Over {
                        val,
                        node: e,
                        scope,
                        vals: shared.clone(),
                        env: self.env(),
                        cli_raw: None,
                        cli_indices: Vec::new(),
                    },
                );
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
                if l.abs_diff(r) > MAX_ITER as u128 {
                    return;
                }
                let idx: Vec<i128> = if l >= r { (r..=l).collect() } else { (l..=r).collect() };
                names = names.iter().flat_map(|n| idx.iter().map(move |i| format!("{n}[{i}]"))).collect();
            }
            let port_conns = self.extract_port_conns(inst, vals);
            let loc = self.loc_of(inst);
            let cur_gen = self.cur_gen_prefix();
            for n in names {
                self.out.push(format!("{path} instance {n} : {def}"));
                let mut over = over.clone();
                let mut pend = Vec::new();
                for (segs, v) in self.defparams.last().into_iter().flatten() {
                    let matched_rest = if segs.first() == Some(&n) {
                        Some(&segs[1..])
                    } else if !cur_gen.is_empty() {
                        let gen_segs: Vec<&str> = cur_gen.split('.').collect();
                        if segs.len() > gen_segs.len()
                            && segs.iter().zip(&gen_segs).all(|(a, b)| a == b)
                            && segs.get(gen_segs.len()) == Some(&n)
                        {
                            Some(&segs[gen_segs.len() + 1..])
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    if let Some(after) = matched_rest {
                        match after {
                            [p] => {
                                let mut next = v.clone();
                                if let Some(prev) = over.remove(p) {
                                    next.cli_indices.extend(prev.cli_indices);
                                }
                                over.insert(p.clone(), next);
                            }
                            rest if !rest.is_empty() => pend.push((rest.to_vec(), v.clone())),
                            _ => {}
                        }
                    }
                }
                let env = Rc::new(types.clone());
                self.instance(&def, &n, &format!("{path}.{n}"), &over, pend, env, port_conns.clone(), loc, depth + 1);
            }
        }
    }

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
                                let bad = !v.known() || v.v.is_zero() || (v.s && v.v.is_neg());
                                if bad {
                                    self.checked.insert(n);
                                    self.diags.push(crate::names::Diag {
                                        tree: n.tree,
                                        node: w.node,
                                        message: format!(
                                            "indexed part-select width must be positive, not {}",
                                            v.show()
                                        ),
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

    fn defparam(&mut self, c: NodeRef, scope: usize, vals: &Vals) {
        let b = self.b;
        let kids = b.children(c);
        let (Some(&target), Some(&e)) = (kids.first(), kids.get(1)) else { return };
        let segs: Vec<String> = b.toks_of(target).split(|t| *t == ".").map(|p| p.concat()).collect();
        if segs.len() < 2 {
            return;
        }
        let val = self.eval(e, scope, vals);
        let o =
            Over { val, node: e, scope, vals: vals.clone(), env: self.env(), cli_raw: None, cli_indices: Vec::new() };
        if let Some(d) = self.defparams.last_mut() {
            d.push((segs, o));
        }
    }

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

fn parse_port_dir(tok: &str) -> Option<PortDir> {
    match tok {
        "input" => Some(PortDir::Input),
        "output" => Some(PortDir::Output),
        "inout" => Some(PortDir::Inout),
        "ref" => Some(PortDir::Ref),
        _ => None,
    }
}

/// Evaluates a CLI `-G <name>=<raw>` SystemVerilog constant expression in isolation,
/// rejecting syntax errors or undeclared identifiers fail-closed.
fn eval_cli_expr(raw: &str, shape: Option<(u32, bool)>, two_state: bool) -> Option<(CVal, String)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let dt_str = match shape {
        Some((w, s)) => {
            let kw = if two_state { "bit" } else { "logic" };
            let sg = if s { " signed" } else { "" };
            format!("{kw}{sg} [{}:0]", w.max(1) - 1)
        }
        None => String::new(),
    };
    let src = if dt_str.is_empty() {
        format!("module __svlang_cli_param; parameter __v = {trimmed}; endmodule\n")
    } else {
        format!("module __svlang_cli_param; parameter {dt_str} __v = {trimmed}; endmodule\n")
    };
    let tree = sv_syntax::parse_source(&src, "<cli-param>", sv_syntax::Options::default());
    if !tree.errors.is_empty() || !tree.pp_diagnostics.is_empty() {
        return None;
    }
    let trees = [tree];
    let scopes = crate::scopes::build(&trees);
    if !crate::names::check(&trees, &scopes).is_empty() {
        return None;
    }
    let mut b = Builder::new(&trees);
    b.designs = true;
    b.collect_names();
    b.collect();
    let p = trees[0].preorder().into_iter().find(|&n| trees[0].kind(n) == ParamDecl)?;
    let pr = NodeRef { tree: 0, node: p };
    let dt = b.child(pr, DataType);
    let d = b.children(pr).into_iter().find(|c| b.kind(*c) == Declarator)?;
    let v = b.children(d).into_iter().find(|c| !matches!(b.kind(*c), Name | Dim))?;
    let empty: Env = Rc::new(HashMap::default());
    let val = b.eval_xz(v, b.unit, &empty, two_state).map(|c| match shape {
        Some((w, s)) if c.w < w => {
            let x = b.eval_ctx(v, w, s, b.unit, &empty, 0).unwrap_or_else(|| c.cast(w, s));
            if two_state {
                x.two_state()
            } else {
                x
            }
        }
        Some((w, s)) => c.cast(w, s),
        None => c,
    })?;
    let text = b.value_text(dt, v, b.unit, &empty).unwrap_or_else(|| val.show());
    Some((val, text))
}
