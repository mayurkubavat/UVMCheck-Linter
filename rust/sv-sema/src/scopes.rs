//! Scopes for every design unit (builder phase N1): modules, interfaces,
//! programs, packages, `$unit`, classes and subroutines, each with the
//! symbols it declares. This is the symbol table the builder resolves names
//! in. Generate blocks and statement blocks are elaboration's (phase N4).
//!
//! [`Scopes::dump`] prints it in the form of the Slang reference
//! (tools/sym_dump.cpp), one `<scope path> <kind> <name>` line per symbol.

use std::collections::{HashMap, HashSet};

use sv_syntax::{Child, NodeId, SyntaxKind, Tree};
use SyntaxKind::*;

/// What a symbol is.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum SymKind {
    Module,
    Interface,
    Program,
    Package,
    Class,
    Parameter,
    TypeParameter,
    Port,
    Net,
    Variable,
    Typedef,
    EnumValue,
    Function,
    Task,
    Argument,
    Constraint,
    Covergroup,
    Modport,
    Clocking,
    Genvar,
    Sequence,
    Property,
    Let,
    Nettype,
    Specparam,
    Instance,
}

impl SymKind {
    pub fn as_str(self) -> &'static str {
        use SymKind::*;
        match self {
            Module => "module",
            Interface => "interface",
            Program => "program",
            Package => "package",
            Class => "class",
            Parameter => "parameter",
            TypeParameter => "type_parameter",
            Port => "port",
            Net => "net",
            Variable => "variable",
            Typedef => "typedef",
            EnumValue => "enum_value",
            Function => "function",
            Task => "task",
            Argument => "argument",
            Constraint => "constraint",
            Covergroup => "covergroup",
            Modport => "modport",
            Clocking => "clocking",
            Genvar => "genvar",
            Sequence => "sequence",
            Property => "property",
            Let => "let",
            Nettype => "nettype",
            Specparam => "specparam",
            Instance => "instance",
        }
    }
}

/// A declared name.
#[derive(Clone, Debug)]
pub struct Symbol {
    /// The scope that declares it.
    pub scope: usize,
    pub kind: SymKind,
    pub name: String,
    /// Tree index and the node that declares it (its Name, or the
    /// declaration itself when the name is a bare token).
    pub tree: usize,
    pub node: NodeId,
}

/// A scope: the design root, `$unit`, a design unit, class or subroutine.
#[derive(Clone, Debug)]
pub struct Scope {
    /// Dotted path from the root (`m`, `p.c`, `$unit.c.f`); `$root` for the
    /// root that holds the design units.
    pub path: String,
    pub parent: Option<usize>,
    /// The symbol that opens it (none for `$root` and `$unit`).
    pub symbol: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct Scopes {
    pub scopes: Vec<Scope>,
    pub symbols: Vec<Symbol>,
}

pub const ROOT: usize = 0;
pub const UNIT: usize = 1;

impl Scopes {
    /// The Slang-reference form: one sorted `<path> <kind> <name>` line per symbol.
    pub fn dump(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .symbols
            .iter()
            .map(|s| format!("{} {} {}", self.scopes[s.scope].path, s.kind.as_str(), s.name))
            .collect();
        v.sort();
        v
    }
}

/// Builds the scopes of already parsed trees (one per compilation unit).
pub fn build(trees: &[Tree]) -> Scopes {
    let mut out = Scopes::default();
    out.scopes.push(Scope { path: "$root".into(), parent: None, symbol: None });
    out.scopes.push(Scope { path: "$unit".into(), parent: None, symbol: None });
    // Interface names, to tell an interface port (`bus b`) from a typed
    // one; definition names, for bind; net types, to tell nets from variables.
    let mut names = Names::default();
    for t in trees {
        for n in t.preorder() {
            match t.kind(n) {
                InterfaceDecl => {
                    names.interfaces.insert(name_of(t, n));
                    names.definitions.insert(name_of(t, n));
                }
                ModuleDecl | ProgramDecl => {
                    names.definitions.insert(name_of(t, n));
                }
                NetTypeDecl => {
                    names.nettypes.insert(name_of(t, n));
                }
                _ => {}
            }
        }
    }
    let mut binds = Vec::new();
    for (ix, t) in trees.iter().enumerate() {
        let mut w = Walker { t, ix, out: &mut out, names: &names, binds: &mut binds };
        for c in t.children(t.root) {
            w.item(UNIT, c, Ctx::Unit);
        }
    }
    // `bind target type name(...)` adds instances to every target definition.
    for (target, inst) in binds {
        let scope = out.scopes.iter().position(|s| {
            s.parent == Some(ROOT)
                && s.path == target
                && s.symbol.is_some_and(|y| {
                    matches!(out.symbols[y].kind, SymKind::Module | SymKind::Interface | SymKind::Program)
                })
        });
        if let Some(scope) = scope {
            for mut sym in inst {
                sym.scope = scope;
                out.symbols.push(sym);
            }
        }
    }
    out
}

#[derive(Default)]
struct Names {
    interfaces: HashSet<String>,
    definitions: HashSet<String>,
    nettypes: HashSet<String>,
}

/// Where an item sits; decides what a declaration means.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ctx {
    Unit,
    Package,
    Design,
    Class,
    Subroutine,
}

struct Walker<'a, 't> {
    t: &'t Tree,
    ix: usize,
    out: &'a mut Scopes,
    names: &'a Names,
    /// Bind directives: target definition and the instances to add there.
    binds: &'a mut Vec<(String, Vec<Symbol>)>,
}

fn name_of(t: &Tree, n: NodeId) -> String {
    t.child(n, Name).map(|c| t.bare_text(c)).unwrap_or_default()
}

/// Texts of a node's own tokens (not its children's).
fn toks(t: &Tree, n: NodeId) -> Vec<&str> {
    t.node(n)
        .children
        .iter()
        .filter_map(|c| match c {
            Child::Token(i) => Some(&*t.tokens[*i as usize].text),
            Child::Node(_) => None,
        })
        .collect()
}

fn first_tok(t: &Tree, n: NodeId) -> Option<&str> {
    t.token_bounds(n).map(|(f, _)| &*t.tokens[f as usize].text)
}

const NET_TYPES: &[&str] = &[
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

const DIRECTIONS: &[&str] = &["input", "output", "inout", "ref"];

/// A data type written out (not just `signed` or a packed range).
fn explicit_type(t: &Tree, dt: Option<NodeId>) -> bool {
    dt.and_then(|d| first_tok(t, d)).is_some_and(|f| !matches!(f, "[" | "signed" | "unsigned"))
}

impl Walker<'_, '_> {
    fn add(&mut self, scope: usize, kind: SymKind, mut name: String, node: NodeId) -> usize {
        // An escaped identifier names what follows the backslash.
        if name.starts_with('\\') {
            name.remove(0);
        }
        self.out.symbols.push(Symbol { scope, kind, name, tree: self.ix, node });
        self.out.symbols.len() - 1
    }

    fn open(&mut self, parent: usize, kind: SymKind, name: String, node: NodeId) -> usize {
        let sym = self.add(parent, kind, name.clone(), node);
        let path = if parent == ROOT { name } else { format!("{}.{}", self.out.scopes[parent].path, name) };
        self.out.scopes.push(Scope { path, parent: Some(parent), symbol: Some(sym) });
        self.out.scopes.len() - 1
    }

    /// Items of a declaration: its children other than the name, header and labels.
    fn body_items(&self, n: NodeId) -> Vec<NodeId> {
        self.t.children(n).filter(|&c| !matches!(self.t.kind(c), Name | Header | EndLabel | Attribute)).collect()
    }

    fn item(&mut self, scope: usize, n: NodeId, ctx: Ctx) {
        let t = self.t;
        match t.kind(n) {
            // A nested definition is named at the root as well.
            ModuleDecl | InterfaceDecl | ProgramDecl if matches!(ctx, Ctx::Unit | Ctx::Design) => {
                let kind = match t.kind(n) {
                    ModuleDecl => SymKind::Module,
                    InterfaceDecl => SymKind::Interface,
                    _ => SymKind::Program,
                };
                let name = name_of(t, n);
                if name.is_empty() {
                    return;
                }
                let s = self.open(ROOT, kind, name, n);
                self.design_unit(s, n);
            }
            PackageDecl if ctx == Ctx::Unit => {
                let s = self.open(ROOT, SymKind::Package, name_of(t, n), n);
                for c in self.body_items(n) {
                    self.item(s, c, Ctx::Package);
                }
            }
            ClassDecl => {
                let s = self.open(scope, SymKind::Class, name_of(t, n), n);
                if let Some(pl) = t.child(n, Header).and_then(|h| t.child(h, ParamList)) {
                    self.params(s, pl);
                }
                for c in self.body_items(n) {
                    self.item(s, c, Ctx::Class);
                }
            }
            FunctionDecl | TaskDecl => {
                let name = name_of(t, n);
                // An out-of-block body (`c::f`) belongs to its class's prototype.
                if name.is_empty() || name.contains("::") {
                    return;
                }
                let kind = if t.kind(n) == TaskDecl { SymKind::Task } else { SymKind::Function };
                let s = self.open(scope, kind, name, n);
                self.subroutine(s, n);
            }
            MethodPrototype => {
                let is_task = toks(t, n).contains(&"task");
                let kind = if is_task { SymKind::Task } else { SymKind::Function };
                self.add(scope, kind, name_of(t, n), n);
            }
            DpiImport => {
                let name = name_of(t, n);
                if name.is_empty() {
                    return;
                }
                let is_task = toks(t, n).contains(&"task");
                let s = self.open(scope, if is_task { SymKind::Task } else { SymKind::Function }, name, n);
                self.subroutine(s, n);
            }
            ParamDecl => {
                self.param_decl(scope, n, None);
            }
            TypedefDecl => {
                self.enum_values(scope, n);
                let name = name_of(t, n);
                if !name.is_empty() {
                    self.add(scope, SymKind::Typedef, name, n);
                }
            }
            DataDecl => self.data_decl(scope, n, ctx),
            ConstraintDecl | ConstraintPrototype => {
                let name = name_of(t, n);
                if !name.contains("::") {
                    self.add(scope, SymKind::Constraint, name, n);
                }
            }
            CovergroupDecl => {
                // In a class it declares an anonymous type and a variable.
                let kind = if ctx == Ctx::Class { SymKind::Variable } else { SymKind::Covergroup };
                self.add(scope, kind, name_of(t, n), n);
            }
            ModportDecl => {
                for d in t.children(n).filter(|&c| t.kind(c) == Declarator) {
                    if let Some(nm) = t.child(d, Name) {
                        self.add(scope, SymKind::Modport, t.bare_text(nm), d);
                    }
                }
            }
            ClockingDecl => {
                let name = name_of(t, n);
                if !name.is_empty() {
                    self.add(scope, SymKind::Clocking, name, n);
                }
            }
            GenvarDecl => {
                for tok in toks(t, n) {
                    if tok != "genvar" && tok != "," && tok != ";" {
                        self.add(scope, SymKind::Genvar, tok.to_string(), n);
                    }
                }
            }
            SequenceDecl => self.named(scope, n, SymKind::Sequence),
            PropertyDecl => self.named(scope, n, SymKind::Property),
            LetDecl => self.named(scope, n, SymKind::Let),
            NetTypeDecl => self.named(scope, n, SymKind::Nettype),
            SpecparamDecl => {
                for d in t.children(n).filter(|&c| t.kind(c) == Declarator) {
                    self.add(scope, SymKind::Specparam, name_of(t, d), d);
                }
            }
            Instantiation => {
                for i in t.children(n).filter(|&c| t.kind(c) == Instance) {
                    if let Some(nm) = t.child(i, Name) {
                        self.add(scope, SymKind::Instance, t.bare_text(nm), i);
                    }
                }
            }
            GateInst => {
                for i in t.children(n).filter(|&c| t.kind(c) == Instance) {
                    if let Some(nm) = t.child(i, Name) {
                        self.add(scope, SymKind::Instance, t.bare_text(nm), i);
                    }
                }
            }
            BindDirective => {
                // Only `bind target ...`, not `bind target: instances ...`.
                let target = t.child(n, NameExpr);
                let ty = t.child(n, DataType).map(|d| t.bare_text(d));
                if let (Some(target), Some(ty)) = (target, ty) {
                    if !toks(t, n).contains(&":") && self.names.definitions.contains(&ty) {
                        let inst = t
                            .children(n)
                            .filter(|&c| t.kind(c) == Instance)
                            .filter_map(|i| t.child(i, Name))
                            .map(|nm| Symbol {
                                scope,
                                kind: SymKind::Instance,
                                name: t.bare_text(nm),
                                tree: self.ix,
                                node: nm,
                            })
                            .collect();
                        self.binds.push((t.bare_text(target), inst));
                    }
                }
            }
            GenerateRegion => {
                for c in self.body_items(n) {
                    self.item(scope, c, ctx);
                }
            }
            _ => {}
        }
    }

    fn named(&mut self, scope: usize, n: NodeId, kind: SymKind) {
        let name = name_of(self.t, n);
        if !name.is_empty() {
            self.add(scope, kind, name, n);
        }
    }

    fn design_unit(&mut self, s: usize, n: NodeId) {
        let t = self.t;
        let header = t.child(n, Header);
        if let Some(pl) = header.and_then(|h| t.child(h, ParamList)) {
            self.params(s, pl);
        }
        let ports = header.and_then(|h| t.child(h, PortList));
        let ansi = ports.map(|pl| self.is_ansi(pl)).unwrap_or(true);
        if let Some(pl) = ports {
            if ansi {
                self.ansi_ports(s, pl);
            } else {
                for p in t.children(pl).filter(|&c| t.kind(c) == Port) {
                    if let Some(name) = self.non_ansi_port_name(p) {
                        self.add(s, SymKind::Port, name, p);
                    }
                }
            }
        }
        // Non-ANSI port declarations name a net or variable, unless a
        // separate declaration of the same name does.
        let items = self.body_items(n);
        let mut declared = HashSet::new();
        for &c in &items {
            if t.kind(c) == DataDecl && !self.has_direction(c) {
                for d in t.children(c).filter(|&d| t.kind(d) == Declarator) {
                    declared.insert(name_of(t, d));
                }
            }
        }
        for c in items {
            if t.kind(c) == DataDecl && self.has_direction(c) {
                let kind = self.port_decl_kind(c);
                for d in t.children(c).filter(|&d| t.kind(d) == Declarator) {
                    let name = name_of(t, d);
                    if !declared.contains(&name) {
                        self.add(s, kind, name, d);
                    }
                }
                continue;
            }
            self.item(s, c, Ctx::Design);
        }
    }

    fn has_direction(&self, n: NodeId) -> bool {
        toks(self.t, n).first().is_some_and(|f| DIRECTIONS.contains(f))
    }

    /// The internal symbol of a non-ANSI port declaration.
    fn port_decl_kind(&self, n: NodeId) -> SymKind {
        let tk = toks(self.t, n);
        if tk.iter().any(|x| NET_TYPES.contains(x)) {
            return SymKind::Net;
        }
        if tk.iter().any(|&x| x == "var" || x == "reg") || explicit_type(self.t, self.t.child(n, DataType)) {
            return SymKind::Variable;
        }
        SymKind::Net
    }

    fn is_ansi(&self, pl: NodeId) -> bool {
        let t = self.t;
        let Some(p) = t.children(pl).find(|&c| t.kind(c) == Port) else {
            return true;
        };
        let kids: Vec<NodeId> = t.children(p).collect();
        let bare = toks(t, p).is_empty() && kids.len() == 1 && t.kind(kids[0]) == Declarator && {
            let d = kids[0];
            t.children(d).count() == 1 && t.child(d, Name).is_some()
        };
        let lead = first_tok(t, p);
        !(bare || lead == Some(".") || lead == Some("{"))
    }

    fn non_ansi_port_name(&self, p: NodeId) -> Option<String> {
        let t = self.t;
        if toks(t, p).first() == Some(&".") {
            return Some(name_of(t, p));
        }
        if first_tok(t, p) == Some("{") {
            return None;
        }
        let d = t.child(p, Declarator)?;
        Some(name_of(t, d))
    }

    fn ansi_ports(&mut self, s: usize, pl: NodeId) {
        let t = self.t;
        // Direction and kind carried from the previous port.
        let mut dir = "inout";
        let mut kind = SymKind::Net;
        for p in t.children(pl).filter(|&c| t.kind(c) == Port) {
            let tk = toks(t, p);
            let dt = t.child(p, DataType);
            let Some(d) = t.child(p, Declarator) else {
                // `.name(expr)`: a port with no internal symbol.
                if tk.first() == Some(&".") {
                    self.add(s, SymKind::Port, name_of(t, p), p);
                }
                continue;
            };
            let name = name_of(t, d);
            let iface = tk.first() == Some(&"interface")
                || (dt.is_none() && tk.get(1) == Some(&".") && !DIRECTIONS.contains(&tk[0]))
                || (!tk.iter().any(|x| DIRECTIONS.contains(x))
                    && dt.is_some_and(|d| self.names.interfaces.contains(&t.bare_text(d))));
            self.add(s, SymKind::Port, name.clone(), d);
            if iface {
                continue;
            }
            let new_dir = tk.iter().find(|x| DIRECTIONS.contains(x));
            let net_kw = tk.iter().any(|x| NET_TYPES.contains(x));
            let var_kw = tk.contains(&"var");
            if new_dir.is_some() || dt.is_some() || net_kw || var_kw {
                if let Some(d) = new_dir {
                    dir = d;
                }
                kind = if net_kw {
                    SymKind::Net
                } else if var_kw || dir == "ref" || (dir == "output" && explicit_type(t, dt)) {
                    SymKind::Variable
                } else {
                    SymKind::Net
                };
            }
            self.add(s, kind, name, d);
        }
    }

    /// The parameters of a `#(...)` list; a parameter with no keyword
    /// continues the previous one's kind.
    fn params(&mut self, s: usize, pl: NodeId) {
        let mut ty = false;
        for p in self.t.children(pl).filter(|&c| self.t.kind(c) == ParamDecl) {
            ty = self.param_decl(s, p, Some(ty));
        }
    }

    /// Declares a parameter declaration's names; returns whether they are types.
    fn param_decl(&mut self, s: usize, n: NodeId, prev_type: Option<bool>) -> bool {
        let t = self.t;
        let tk = toks(t, n);
        let ty = if tk.contains(&"type") {
            true
        } else if tk.iter().any(|&x| x == "parameter" || x == "localparam") || t.child(n, DataType).is_some() {
            false
        } else {
            prev_type.unwrap_or(false)
        };
        self.enum_values(s, n);
        let kind = if ty { SymKind::TypeParameter } else { SymKind::Parameter };
        for c in t.children(n) {
            match t.kind(c) {
                Name => {
                    self.add(s, kind, t.bare_text(c), c);
                }
                Declarator => {
                    self.add(s, kind, name_of(t, c), c);
                }
                _ => {}
            }
        }
        ty
    }

    fn data_decl(&mut self, s: usize, n: NodeId, ctx: Ctx) {
        let t = self.t;
        self.enum_values(s, n);
        let tk = toks(t, n);
        let kind = if ctx == Ctx::Subroutine && tk.first().is_some_and(|f| DIRECTIONS.contains(f)) {
            SymKind::Argument
        } else if tk.iter().any(|x| NET_TYPES.contains(x))
            || t.child(n, DataType).is_some_and(|d| self.names.nettypes.contains(&t.bare_text(d)))
        {
            SymKind::Net
        } else {
            SymKind::Variable
        };
        for d in t.children(n).filter(|&c| t.kind(c) == Declarator) {
            let name = name_of(t, d);
            if !name.is_empty() {
                self.add(s, kind, name, d);
            }
        }
    }

    fn subroutine(&mut self, s: usize, n: NodeId) {
        let t = self.t;
        if let Some(pl) = t.child(n, PortList) {
            for p in t.children(pl).filter(|&c| t.kind(c) == Port) {
                self.enum_values(s, p);
                if let Some(d) = t.child(p, Declarator) {
                    self.add(s, SymKind::Argument, name_of(t, d), d);
                }
            }
        }
        if let Some(b) = t.child(n, Body) {
            for c in t.children(b) {
                if matches!(t.kind(c), DataDecl | TypedefDecl | ParamDecl) {
                    self.item(s, c, Ctx::Subroutine);
                }
            }
        }
    }

    /// Enum names a declaration's types introduce (not those of struct members).
    fn enum_values(&mut self, s: usize, n: NodeId) {
        let t = self.t;
        let mut stack: Vec<NodeId> = t.children(n).filter(|&c| t.kind(c) == DataType).collect();
        for c in t.children(n).filter(|&c| t.kind(c) == Declarator) {
            stack.extend(t.children(c).filter(|&d| t.kind(d) == DataType));
        }
        while let Some(dt) = stack.pop() {
            match first_tok(t, dt) {
                Some("enum") => {
                    for d in t.children(dt).filter(|&c| t.kind(c) == Declarator) {
                        self.enum_value(s, d);
                    }
                    stack.extend(t.children(dt).filter(|&c| t.kind(c) == DataType));
                }
                Some("struct" | "union") => {}
                _ => stack.extend(t.children(dt).filter(|&c| t.kind(c) == DataType)),
            }
        }
    }

    /// `A`, or `A[2]` (A0, A1) and `A[1:3]` (A1, A2, A3).
    fn enum_value(&mut self, s: usize, d: NodeId) {
        let t = self.t;
        let name = name_of(t, d);
        if let Some(dim) = t.child(d, Dim) {
            let nums: Vec<i64> =
                t.token_range(dim).iter().filter_map(|&i| t.tokens[i as usize].text.parse().ok()).collect();
            let range: Vec<i64> = match nums[..] {
                [n] => (0..n).collect(),
                [a, b] if a <= b => (a..=b).collect(),
                [a, b] => (b..=a).rev().collect(),
                _ => vec![],
            };
            for i in range {
                self.add(s, SymKind::EnumValue, format!("{name}{i}"), d);
            }
            return;
        }
        self.add(s, SymKind::EnumValue, name, d);
    }
}

/// Symbols of each scope by name, in declaration order.
pub fn by_scope(s: &Scopes) -> HashMap<usize, Vec<usize>> {
    let mut m: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, sym) in s.symbols.iter().enumerate() {
        m.entry(sym.scope).or_default().push(i);
    }
    m
}
