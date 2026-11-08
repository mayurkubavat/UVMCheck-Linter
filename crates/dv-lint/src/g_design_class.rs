//! Group D: a class index over one syntax tree, standing in for slang's
//! class symbols (`swe::userClasses`, `classMethods`, `classRole`,
//! `getOverride`), and the rules that read classes and their methods.

use std::collections::{BTreeMap, HashMap, HashSet};

use sv_syntax::{Child, NodeId, SyntaxKind};

use crate::g_design::*;
use crate::syntax::Cst;
use crate::Check::Syntax;
use crate::{Rule, Severity, Sink};
use SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule {
            id: "SV-SML-001",
            name: "ScratchpadClassProperty",
            check: Syntax(scratchpad_property),
        },
        Rule {
            id: "SV-SML-002",
            name: "MagicDelayInSequence",
            check: Syntax(magic_delay_in_sequence),
        },
        Rule {
            id: "SV-MET-008",
            name: "RandFieldBudget",
            check: Syntax(rand_field_budget),
        },
        Rule {
            id: "SV-DSN-006",
            name: "OverrideOnlyCallsSuper",
            check: Syntax(override_only_calls_super),
        },
        Rule {
            id: "SV-DSN-007",
            name: "EmptyOverrideOfConcrete",
            check: Syntax(empty_override),
        },
        Rule {
            id: "SV-DSN-008",
            name: "MutablePackageState",
            check: Syntax(mutable_package_state),
        },
        Rule {
            id: "SV-DSN-010",
            name: "QueryChangesState",
            check: Syntax(query_changes_state),
        },
        Rule {
            id: "SV-DSN-012",
            name: "PrimitiveObsession",
            check: Syntax(primitive_obsession),
        },
        Rule {
            id: "SV-DSN-013",
            name: "FatalStubOverride",
            check: Syntax(fatal_stub_override),
        },
        Rule {
            id: "UVM-ROL-007",
            name: "ProceduralTimeoutInMonitor",
            check: Syntax(procedural_timeout),
        },
        Rule {
            id: "UVM-CFG-006",
            name: "PlusargsOutsideTest",
            check: Syntax(plusargs_outside_test),
        },
        Rule {
            id: "UVM-ARC-004",
            name: "StringHierarchyLookup",
            check: Syntax(string_hierarchy_lookup),
        },
        Rule {
            id: "UVM-ARC-003",
            name: "FileIncludedInMultiplePackages",
            check: Syntax(file_in_many_packages),
        },
    ]
}

// ---- the index ----

pub(crate) struct Prop {
    pub name: String,
    pub tok: u32,
    pub ty: Option<NodeId>,
    /// Declared with unpacked dimensions.
    pub array: bool,
    pub rand: bool,
    pub is_static: bool,
    pub is_const: bool,
}

pub(crate) struct Port {
    pub name: String,
    pub ty: Option<NodeId>,
    pub dir: &'static str,
    pub is_const: bool,
}

pub(crate) struct Meth {
    pub name: String,
    /// The name token of the declaration with the body (slang's location).
    pub tok: u32,
    /// FunctionDecl or TaskDecl holding the body.
    pub decl: NodeId,
    pub body: Option<NodeId>,
    pub is_task: bool,
    pub virtual_kw: bool,
    pub ports: Vec<Port>,
    /// Defined outside the class (`function void C::m()`).
    pub out_of_block: bool,
}

pub(crate) struct Class {
    pub node: NodeId,
    pub name: String,
    pub tok: u32,
    pub base: Option<String>,
    pub generic: bool,
    pub library: bool,
    pub props: Vec<Prop>,
    pub methods: Vec<Meth>,
    /// Pure virtual prototypes, by name (they have no body).
    pub pure: Vec<String>,
}

pub(crate) struct Classes {
    pub list: Vec<Class>,
    by_name: HashMap<String, Vec<usize>>,
    typedefs: HashMap<String, String>,
    by_node: HashMap<u32, usize>,
}

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

impl Role {
    pub fn name(self) -> &'static str {
        match self {
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

    pub fn is_component(self) -> bool {
        matches!(
            self,
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
}

fn name_says_config(n: &str) -> bool {
    let l = n.to_ascii_lowercase();
    l.contains("cfg") || l.contains("config")
}

/// The type name a DataType stands for: its last identifier before any
/// parameter list (`pkg::cls #(T)` gives `cls`).
pub(crate) fn type_name(c: &Cst, ty: NodeId) -> Option<String> {
    let mut last = None;
    for k in c.kids(ty) {
        match *k {
            Child::Token(t) => {
                let s = c.text_of(t);
                if s.chars().next().is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_') {
                    last = Some(s.to_string());
                }
            }
            Child::Node(_) => {}
        }
    }
    last
}

fn decl_names(c: &Cst, n: NodeId) -> Vec<(String, u32)> {
    c.tree
        .children(n)
        .filter(|&d| c.kind(d) == Declarator)
        .filter_map(|d| {
            let nm = c.tree.child(d, Name)?;
            let t = c.first_tok(nm)?;
            Some((c.text_of(t).to_string(), t))
        })
        .collect()
}

pub(crate) fn ports(c: &Cst, decl: NodeId) -> Vec<Port> {
    let Some(list) = c.tree.child(decl, PortList) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut dir = "input";
    let mut ty: Option<NodeId> = None;
    for p in c.tree.children(list).filter(|&p| c.kind(p) == Port) {
        let toks: Vec<&str> = c.toks(p).map(|t| c.text_of(t)).collect();
        let new_dir = toks
            .iter()
            .find(|t| matches!(**t, "input" | "output" | "inout" | "ref"))
            .copied();
        let pty = c.tree.child(p, DataType);
        if let Some(d) = new_dir {
            dir = match d {
                "input" => "input",
                "output" => "output",
                "inout" => "inout",
                _ => "ref",
            };
            ty = pty;
        } else if pty.is_some() {
            ty = pty;
        }
        let is_const = toks.contains(&"const");
        for (name, _) in decl_names(c, p) {
            out.push(Port {
                name,
                ty,
                dir,
                is_const,
            });
        }
    }
    out
}

fn method_name(c: &Cst, decl: NodeId) -> Option<(String, u32, Option<String>)> {
    let nm = c.tree.child(decl, Name)?;
    let toks: Vec<u32> = c.toks(nm).collect();
    let last = *toks.last()?;
    let scope = (toks.len() >= 3 && c.text_of(toks[toks.len() - 2]) == "::").then(|| c.text_of(toks[0]).to_string());
    Some((c.text_of(last).to_string(), last, scope))
}

fn has_tok(c: &Cst, n: NodeId, s: &str) -> bool {
    c.toks(n).any(|t| c.text_of(t) == s)
}

fn meth(c: &Cst, decl: NodeId, virtual_kw: bool, out_of_block: bool) -> Option<Meth> {
    let (name, tok, _) = method_name(c, decl)?;
    Some(Meth {
        name,
        tok,
        decl,
        body: c.tree.child(decl, Body),
        is_task: c.kind(decl) == TaskDecl,
        virtual_kw: virtual_kw || has_tok(c, decl, "virtual"),
        ports: ports(c, decl),
        out_of_block,
    })
}

impl Classes {
    /// The tree's classes, built once and shared.
    pub fn new(c: &Cst) -> std::sync::Arc<Classes> {
        crate::shared::get("classes", c.tree_key(), || Classes::build(c))
    }

    fn build(c: &Cst) -> Classes {
        let mut list = Vec::new();
        let mut typedefs = HashMap::new();
        for t in c.nodes(TypedefDecl) {
            let (Some(nm), Some(ty)) = (c.tree.child(t, Name), c.tree.child(t, DataType)) else {
                continue;
            };
            if let Some(target) = type_name(c, ty) {
                typedefs.insert(c.tree.bare_text(nm), target);
            }
        }
        // Out-of-block bodies by (class, method).
        let mut bodies: HashMap<(String, String), NodeId> = HashMap::new();
        for d in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)) {
            if let Some((name, _, Some(scope))) = method_name(c, d) {
                bodies.entry((scope, name)).or_insert(d);
            }
        }
        for n in c.nodes(ClassDecl) {
            let Some(nm) = c.tree.child(n, Name) else { continue };
            let Some(tok) = c.first_tok(nm) else { continue };
            let name = c.text_of(tok).to_string();
            let header = c.tree.child(n, Header);
            let base = header
                .and_then(|h| {
                    c.tree
                        .children(h)
                        .find(|&e| c.kind(e) == Extends && has_tok(c, e, "extends"))
                })
                .and_then(|e| c.tree.child(e, DataType))
                .and_then(|ty| type_name(c, ty));
            let generic = header.is_some_and(|h| c.tree.child(h, ParamList).is_some());
            let mut package = None;
            let mut lib_parent = false;
            let mut x = c.parent(n);
            while let Some(y) = x {
                match c.kind(y) {
                    PackageDecl => {
                        if package.is_none() {
                            package = c.tree.child(y, Name).map(|p| c.tree.bare_text(p));
                        }
                    }
                    ClassDecl => {
                        let pn = c.tree.child(y, Name).map(|p| c.tree.bare_text(p)).unwrap_or_default();
                        lib_parent |= pn.starts_with("uvm_");
                    }
                    _ => {}
                }
                x = c.parent(y);
            }
            let file = file_of(c, tok);
            let library = lib_parent
                || name.starts_with("uvm_")
                || package.as_deref() == Some("uvm_pkg")
                || file.contains("uvm_mock.svh")
                || file.contains("uvm_pkg.sv");
            let mut props = Vec::new();
            let mut methods = Vec::new();
            let mut pure = Vec::new();
            for item in c.tree.children(n) {
                match c.kind(item) {
                    DataDecl => {
                        let toks: Vec<&str> = c.toks(item).map(|t| c.text_of(t)).collect();
                        let ty = c.tree.child(item, DataType);
                        for d in c.tree.children(item).filter(|&d| c.kind(d) == Declarator) {
                            let Some(nm) = c.tree.child(d, Name) else { continue };
                            let Some(ptok) = c.first_tok(nm) else { continue };
                            props.push(Prop {
                                name: c.text_of(ptok).to_string(),
                                tok: ptok,
                                ty,
                                array: c.tree.child(d, Dim).is_some(),
                                rand: toks.contains(&"rand") || toks.contains(&"randc"),
                                is_static: toks.contains(&"static"),
                                is_const: toks.contains(&"const"),
                            });
                        }
                    }
                    FunctionDecl | TaskDecl => {
                        if let Some(m) = meth(c, item, false, false) {
                            methods.push(m);
                        }
                    }
                    MethodPrototype => {
                        let Some((mname, _, _)) = method_name(c, item) else {
                            continue;
                        };
                        if has_tok(c, item, "pure") {
                            pure.push(mname);
                            continue;
                        }
                        if has_tok(c, item, "import") {
                            continue;
                        }
                        if let Some(&d) = bodies.get(&(name.clone(), mname)) {
                            if let Some(m) = meth(c, d, has_tok(c, item, "virtual"), true) {
                                methods.push(m);
                            }
                        }
                    }
                    _ => {}
                }
            }
            list.push(Class {
                node: n,
                name,
                tok,
                base,
                generic,
                library,
                props,
                methods,
                pure,
            });
        }
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_node = HashMap::new();
        for (i, k) in list.iter().enumerate() {
            by_name.entry(k.name.clone()).or_default().push(i);
            by_node.insert(k.node.0, i);
        }
        Classes {
            list,
            by_name,
            typedefs,
            by_node,
        }
    }

    /// The class a type name stands for, through typedefs.
    pub fn find(&self, name: &str) -> Option<usize> {
        let mut n = name.to_string();
        for _ in 0..8 {
            if let Some(v) = self.by_name.get(&n) {
                return v.first().copied();
            }
            match self.typedefs.get(&n) {
                Some(t) if *t != n => n = t.clone(),
                _ => return None,
            }
        }
        None
    }

    /// Resolves a type name through typedefs to its final name.
    pub fn resolve_name(&self, name: &str) -> String {
        match self.find(name) {
            Some(k) => self.list[k].name.clone(),
            None => name.to_string(),
        }
    }

    pub fn base(&self, i: usize) -> Option<usize> {
        let b = self.list[i].base.as_deref()?;
        self.find(b).filter(|&j| j != i)
    }

    /// Base class names, nearest first (resolved through typedefs), up to
    /// the first one the index does not hold.
    pub fn chain(&self, i: usize) -> Vec<(String, Option<usize>)> {
        let mut out = Vec::new();
        let mut cur = i;
        for _ in 0..64 {
            let Some(b) = self.list[cur].base.as_deref() else { break };
            let name = self.resolve_name(b);
            match self.find(b).filter(|&j| j != cur) {
                Some(j) => {
                    out.push((self.list[j].name.clone(), Some(j)));
                    cur = j;
                }
                None => {
                    out.push((name, None));
                    break;
                }
            }
        }
        out
    }

    pub fn role(&self, i: usize) -> Role {
        for (n, _) in self.chain(i) {
            if !n.starts_with("uvm_") {
                continue;
            }
            let n = n.as_str();
            let r = if n == "uvm_driver" || n == "uvm_push_driver" {
                Role::Driver
            } else if n == "uvm_monitor" {
                Role::Monitor
            } else if n.starts_with("uvm_sequencer") || n == "uvm_push_sequencer" {
                Role::Sequencer
            } else if n == "uvm_agent" {
                Role::Agent
            } else if n == "uvm_scoreboard" {
                Role::Scoreboard
            } else if n == "uvm_subscriber" {
                Role::Subscriber
            } else if n == "uvm_env" {
                Role::Env
            } else if n == "uvm_test" {
                Role::Test
            } else if n.starts_with("uvm_reg") || n == "uvm_mem" || n == "uvm_vreg" {
                Role::RegModel
            } else if n == "uvm_report_catcher" {
                Role::Catcher
            } else if n.starts_with("uvm_callback") {
                Role::Callback
            } else if n == "uvm_sequence" || n == "uvm_sequence_base" || n == "uvm_reg_sequence" {
                Role::Sequence
            } else if n == "uvm_sequence_item" || n == "uvm_transaction" {
                Role::Item
            } else if n == "uvm_component" || n.starts_with("uvm_tlm") || n == "uvm_root" {
                Role::Component
            } else if n == "uvm_object" || n == "uvm_void" {
                if name_says_config(&self.list[i].name) {
                    Role::Config
                } else {
                    Role::Object
                }
            } else {
                continue;
            };
            return r;
        }
        Role::None
    }

    pub fn uvm_base_name(&self, i: usize) -> Option<String> {
        self.chain(i)
            .into_iter()
            .map(|(n, _)| n)
            .find(|n| n.starts_with("uvm_"))
    }

    /// `swe::derivesFrom(cls, base)`.
    pub fn derives_from(&self, i: usize, base: usize) -> bool {
        self.chain(i).iter().any(|(_, j)| *j == Some(base))
    }

    /// User (non-library) classes.
    pub fn user(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.list.len()).filter(|&i| !self.list[i].library)
    }

    /// Whether a method name is virtual in class `i`: declared so here or in
    /// an ancestor.
    fn is_virtual(&self, i: usize, name: &str) -> bool {
        let mut cur = Some(i);
        let mut guard = 0;
        while let Some(k) = cur {
            guard += 1;
            if guard > 64 {
                break;
            }
            let cls = &self.list[k];
            if cls.pure.iter().any(|p| p == name) {
                return true;
            }
            if let Some(m) = cls.methods.iter().find(|m| m.name == name) {
                if m.virtual_kw {
                    return true;
                }
            }
            cur = self.base(k);
        }
        false
    }

    /// slang's `getOverride()`: the nearest base-class method of the same
    /// name, when that method is virtual. `Err(())` stands for a pure
    /// prototype (no body).
    pub fn override_of(&self, i: usize, name: &str) -> Option<(usize, Result<usize, ()>)> {
        if name == "new" {
            return None;
        }
        let mut cur = self.base(i);
        let mut guard = 0;
        while let Some(k) = cur {
            guard += 1;
            if guard > 64 {
                break;
            }
            let cls = &self.list[k];
            if let Some(mi) = cls.methods.iter().position(|m| m.name == name) {
                return self.is_virtual(k, name).then_some((k, Ok(mi)));
            }
            if cls.pure.iter().any(|p| p == name) {
                return Some((k, Err(())));
            }
            cur = self.base(k);
        }
        None
    }

    /// A base class (at any depth) declares a method or prototype `name`.
    pub fn ancestor_has(&self, i: usize, name: &str) -> bool {
        // Every class has the built-in randomize callbacks.
        if matches!(name, "pre_randomize" | "post_randomize") && self.base(i).is_some() {
            return true;
        }
        let mut cur = self.base(i);
        for _ in 0..64 {
            let Some(k) = cur else { break };
            let cls = &self.list[k];
            if cls.methods.iter().any(|m| m.name == name) || cls.pure.iter().any(|p| p == name) {
                return true;
            }
            cur = self.base(k);
        }
        false
    }

    /// The class whose declaration contains `n`, if any.
    pub fn enclosing(&self, c: &Cst, n: NodeId) -> Option<usize> {
        let mut x = Some(n);
        while let Some(y) = x {
            if let Some(&i) = self.by_node.get(&y.0) {
                return Some(i);
            }
            // An out-of-block method body belongs to its class.
            if matches!(c.kind(y), FunctionDecl | TaskDecl) {
                if let Some((_, _, Some(scope))) = method_name(c, y) {
                    if let Some(i) = self.find(&scope) {
                        return Some(i);
                    }
                }
            }
            x = c.parent(y);
        }
        None
    }

    /// Code slang's `UserCodeVisitor` skips: library classes and uvm_pkg.
    pub fn in_library(&self, c: &Cst, n: NodeId) -> bool {
        if let Some(i) = self.enclosing(c, n) {
            if self.list[i].library {
                return true;
            }
        }
        let mut x = Some(n);
        while let Some(y) = x {
            if c.kind(y) == PackageDecl && c.tree.child(y, Name).is_some_and(|p| c.tree.bare_text(p) == "uvm_pkg") {
                return true;
            }
            x = c.parent(y);
        }
        false
    }
}

// ---- statement helpers shared by class rules ----

/// UVM report levels, weakest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Level {
    None,
    Info,
    Warning,
    Error,
    Fatal,
}

/// `swe::reportLevel` of a call.
pub(crate) fn report_level(c: &Cst, call: NodeId) -> Level {
    match call_name(c, call).unwrap_or("") {
        "$fatal" | "uvm_report_fatal" => Level::Fatal,
        "$error" | "uvm_report_error" => Level::Error,
        "$warning" | "uvm_report_warning" => Level::Warning,
        "$info" | "uvm_report_info" => Level::Info,
        _ => Level::None,
    }
}

/// Calls in a subtree, slang's CallExpressions (system names without
/// parentheses such as `$time` included).
pub(crate) fn calls(c: &Cst, n: NodeId) -> Vec<NodeId> {
    subtree(c, n).into_iter().filter(|&x| c.kind(x) == CallExpr).collect()
}

pub(crate) fn strongest_report(c: &Cst, n: NodeId) -> Level {
    calls(c, n)
        .into_iter()
        .map(|x| report_level(c, x))
        .max()
        .unwrap_or(Level::None)
}

fn is_report_query(n: &str) -> bool {
    matches!(
        n,
        "uvm_report_enabled" | "get_report_verbosity_level" | "get_report_action" | "uvm_get_report_object"
    )
}

/// `swe::isReportGuard`: an if whose condition only asks whether to report.
pub(crate) fn is_report_guard(c: &Cst, n: NodeId) -> bool {
    if c.kind(n) != IfStmt {
        return false;
    }
    let (Some(cond), _, _) = if_parts(c, n) else {
        return false;
    };
    if c.kind(cond) == MatchExpr {
        return false;
    }
    let mut report = false;
    let mut other = false;
    for x in subtree(c, cond) {
        let is_call = c.kind(x) == CallExpr
            || (c.kind(x) == NameExpr
                && c.toks(x).next().is_some_and(|t| c.text_of(t).starts_with('$'))
                && !c.parent(x).is_some_and(|p| c.kind(p) == CallExpr));
        if !is_call {
            continue;
        }
        let name = if c.kind(x) == CallExpr {
            call_name(c, x).unwrap_or("")
        } else {
            c.toks(x).next().map(|t| c.text_of(t)).unwrap_or("")
        };
        if is_report_query(name) {
            report = true;
        } else {
            other = true;
        }
    }
    report && !other
}

fn is_report_local(name: &str) -> bool {
    name.starts_with("_local_report_object_")
}

/// `swe::isReportScaffolding` for a statement node; for a declaration it
/// tells whether the declared name is the report object.
pub(crate) fn is_report_scaffolding(c: &Cst, n: NodeId) -> bool {
    match c.kind(n) {
        DataDecl => c
            .tree
            .children(n)
            .filter(|&d| c.kind(d) == Declarator)
            .filter_map(|d| c.tree.child(d, Name))
            .any(|nm| is_report_local(&c.tree.bare_text(nm))),
        ExprStmt => match assignment(c, n) {
            Some((l, _, "=")) => referenced_name(c, l).is_some_and(is_report_local),
            _ => false,
        },
        _ => false,
    }
}

/// `swe::referencedSymbol`'s name: `x` or the member of `a.x`.
pub(crate) fn referenced_name<'a>(c: &'a Cst, e: NodeId) -> Option<&'a str> {
    let e = strip_parens(c, e);
    match c.kind(e) {
        NameExpr => ident(c, e),
        MemberExpr => {
            let (_, b) = c.tree.token_bounds(e)?;
            Some(c.text_of(b))
        }
        _ => None,
    }
}

/// `swe::countStatements`.
pub(crate) fn count_statements(c: &Cst, n: NodeId) -> usize {
    let mut total = 0;
    for x in list_or_self(c, n) {
        total += count_one(c, x);
    }
    total
}

fn list_or_self(c: &Cst, n: NodeId) -> Vec<NodeId> {
    if c.kind(n) == Body {
        list(c, n)
    } else {
        vec![n]
    }
}

fn count_one(c: &Cst, n: NodeId) -> usize {
    let Some(k) = stmt(c, n) else { return 0 };
    if is_report_scaffolding(c, n) {
        return 0;
    }
    match k {
        St::Decl => c.tree.children(n).filter(|&d| c.kind(d) == Declarator).count(),
        St::If if is_report_guard(c, n) => if_parts(c, n).1.map(|t| count_one(c, t)).unwrap_or(0),
        St::Seq | St::Par(_) => list(c, n).into_iter().map(|x| count_one(c, x)).sum(),
        St::Empty => 0,
        _ => {
            let mut total = 1;
            for x in nested_stmts(c, n) {
                total += count_one(c, x);
            }
            total
        }
    }
}

/// Statements directly inside a statement (branches, bodies, case items).
pub(crate) fn nested_stmts(c: &Cst, n: NodeId) -> Vec<NodeId> {
    match stmt(c, n) {
        Some(St::Seq | St::Par(_)) => list(c, n),
        Some(St::If | St::Assert) => {
            let mut v = sub_stmts(c, n);
            if let Some(e) = c.tree.child(n, ElseClause) {
                v.extend(sub_stmts(c, e));
            }
            v
        }
        Some(St::Case | St::RandCase) => {
            let p = case_parts(c, n);
            let mut v: Vec<NodeId> = p.items.iter().map(|(_, s)| *s).collect();
            v.extend(p.default);
            v
        }
        Some(St::For) => for_parts(c, n).body.into_iter().collect(),
        Some(_) => sub_stmts(c, n),
        None => Vec::new(),
    }
}

/// Every statement under `n` (itself included), as slang's visitor sees them.
pub(crate) fn all_stmts(c: &Cst, n: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = list_or_self(c, n);
    stack.reverse();
    while let Some(x) = stack.pop() {
        out.push(x);
        let mut kids = nested_stmts(c, x);
        kids.reverse();
        stack.extend(kids);
    }
    out
}

// ---- SV-SML-001 ----

fn scratchpad_property(c: &Cst, s: &mut Sink) {
    let units = Units::new(c);
    let classes = Classes::new(c);
    // slang visits members in order and a property already reported as a
    // loop counter is not reported again, so walk loops and properties in
    // source order.
    enum Ev {
        Loop(NodeId),
        Prop(usize, usize),
    }
    let mut events: Vec<(u32, Ev)> = Vec::new();
    for n in c.nodes(LoopStmt) {
        if stmt(c, n) == Some(St::For) {
            events.push((c.first_tok(n).unwrap_or(0), Ev::Loop(n)));
        }
    }
    for (i, cls) in classes.list.iter().enumerate() {
        for (pi, p) in cls.props.iter().enumerate() {
            events.push((p.tok, Ev::Prop(i, pi)));
        }
    }
    events.sort_by_key(|e| e.0);
    let mut flagged: HashSet<(usize, String)> = HashSet::new();
    for (_, ev) in events {
        match ev {
            Ev::Loop(n) => {
                let Some(ci) = classes.enclosing(c, n) else { continue };
                let Some(r) = routine_of(c, n) else { continue };
                for init in for_parts(c, n).init {
                    let Some((l, _, "=")) = assign_expr(c, init) else {
                        continue;
                    };
                    let l = strip_parens(c, l);
                    let name = match c.kind(l) {
                        NameExpr => match ident(c, l) {
                            Some(x) if !local_or_port(c, r, x) => x,
                            _ => continue,
                        },
                        MemberExpr if c.tree.bare_text(l).starts_with("this.") => match referenced_name(c, l) {
                            Some(x) => x,
                            None => continue,
                        },
                        _ => continue,
                    };
                    let Some(owner) = property_owner(&classes, ci, name) else {
                        continue;
                    };
                    report(
                        s,
                        units.times(c, n),
                        "SV-SML-001",
                        "ScratchpadClassProperty",
                        Severity::Warning,
                        format!(
                            "Loop induction variable '{name}' is declared as a class property rather than scoped locally to the loop or routine ('for (int {name} = 0; ...)'). \
                             Class-scoped loop counters introduce severe multi-threaded race conditions when routines execute concurrently in fork..join."
                        ),
                        start(c, init),
                    );
                    flagged.insert((owner, name.to_string()));
                }
            }
            Ev::Prop(i, pi) => {
                let cls = &classes.list[i];
                let p = &cls.props[pi];
                if !matches!(
                    p.name.as_str(),
                    "i" | "j" | "k" | "idx" | "index" | "temp" | "tmp" | "scratch" | "scratchpad" | "loop_cnt"
                ) {
                    continue;
                }
                if flagged.contains(&(i, p.name.clone())) || p.is_static || p.is_const {
                    continue;
                }
                report(
                    s,
                    units.times(c, cls.node),
                    "SV-SML-001",
                    "ScratchpadClassProperty",
                    Severity::Warning,
                    format!(
                        "Scratchpad variable '{}' is declared as a class property. Temporary variables and loop indices must be declared locally within the routine stack \
                         to prevent heap bloat and re-entrancy race conditions.",
                        p.name
                    ),
                    c.loc(p.tok),
                );
            }
        }
    }
}

/// The routine declaration (function or task) around a node.
pub(crate) fn routine_of(c: &Cst, n: NodeId) -> Option<NodeId> {
    let mut x = c.parent(n);
    while let Some(y) = x {
        if matches!(c.kind(y), FunctionDecl | TaskDecl) {
            return Some(y);
        }
        if c.kind(y) == ClassDecl {
            return None;
        }
        x = c.parent(y);
    }
    None
}

/// A local variable or argument of a routine (or a for-loop variable)
/// named `name`, which hides a class property of the same name.
pub(crate) fn local_or_port(c: &Cst, r: NodeId, name: &str) -> bool {
    if ports(c, r).iter().any(|p| p.name == name) {
        return true;
    }
    // A function's implicit return variable.
    if c.kind(r) == FunctionDecl && method_name(c, r).is_some_and(|(n, _, _)| n == name) {
        return true;
    }
    subtree(c, r)
        .into_iter()
        .filter(|&x| c.kind(x) == DataDecl)
        .any(|d| decl_names(c, d).iter().any(|(n, _)| n == name))
}

/// The class (this one or an ancestor) declaring property `name`.
pub(crate) fn property_owner(classes: &Classes, i: usize, name: &str) -> Option<usize> {
    let mut cur = Some(i);
    let mut guard = 0;
    while let Some(k) = cur {
        guard += 1;
        if guard > 64 {
            break;
        }
        if classes.list[k].props.iter().any(|p| p.name == name) {
            return Some(k);
        }
        cur = classes.base(k);
    }
    None
}

// ---- SV-SML-002 ----

fn magic_delay_in_sequence(c: &Cst, s: &mut Sink) {
    let units = Units::new(c);
    let classes = Classes::new(c);
    for n in c.nodes(TimedStmt) {
        let Some(tc) = c.tree.child(n, TimingControl) else {
            continue;
        };
        if c.keyword(tc).map(|t| c.text_of(t)) != Some("#") {
            continue;
        }
        let Some(ci) = classes.enclosing(c, n) else { continue };
        if !is_sequence(&classes, ci) {
            continue;
        }
        let Some(expr) = c.tree.children(tc).next() else {
            continue;
        };
        let zero = c.kind(expr) == Literal && literal_is_zero(&c.trimmed(expr));
        if zero {
            continue;
        }
        // slang's `toString()` of the delay expression.
        let text = c.text(expr);
        report(
            s,
            units.times(c, n),
            "SV-SML-002",
            "MagicDelayInSequence",
            Severity::Warning,
            format!(
                "Hardcoded physical time delay '#{text}' inside uvm_sequence. \
                 Sequences must be clock-frequency agnostic to ensure portability across emulation, FPGA prototyping, and ASIC frequencies. \
                 Synchronize using clocking blocks ('repeat(N) @(vif.cb)') or protocol handshakes ('wait(vif.ready)') instead."
            ),
            start(c, n),
        );
    }
}

/// An integer literal whose value is 0 (time literals such as `0ns` are
/// another kind in slang).
fn literal_is_zero(t: &str) -> bool {
    let t = t.replace('_', "");
    if t.chars().all(|ch| ch.is_ascii_digit()) {
        return t.chars().all(|ch| ch == '0');
    }
    if let Some(p) = t.find('\'') {
        let digits: String = t[p + 1..]
            .chars()
            .skip_while(|ch| matches!(ch, 's' | 'S'))
            .skip(1)
            .collect();
        return !digits.is_empty() && digits.chars().all(|ch| ch == '0');
    }
    false
}

/// `UvmModel::isUvmSequence`: the class derives from uvm_sequence_base or
/// uvm_sequence (by name).
fn is_sequence(classes: &Classes, i: usize) -> bool {
    classes
        .chain(i)
        .iter()
        .any(|(n, _)| matches!(n.as_str(), "uvm_sequence" | "uvm_sequence_base" | "uvm_reg_sequence"))
}

// ---- SV-MET-008 ----

fn rand_field_budget(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    let limit = 8;
    for i in classes.user() {
        if classes.role(i) != Role::Item {
            continue;
        }
        let cls = &classes.list[i];
        let mut n = 0;
        let mut loc = c.loc(cls.tok);
        for item in c.tree.children(cls.node).filter(|&x| c.kind(x) == DataDecl) {
            if !c.toks(item).any(|t| matches!(c.text_of(t), "rand" | "randc")) {
                continue;
            }
            for (_, tok) in decl_names(c, item) {
                n += 1;
                if n == limit + 1 {
                    loc = c.loc(tok);
                }
            }
        }
        if n <= limit {
            continue;
        }
        s.add(
            "SV-MET-008",
            "RandFieldBudget",
            Severity::Warning,
            format!(
                "Sequence item '{}' declares {n} rand fields (limit {limit}). Split the item, or mark fields that the solver should not choose as non-rand.",
                cls.name
            ),
            loc,
        );
    }
}

// ---- SV-DSN-006 ----

/// The only statement of a body, looking through begin/end.
fn sole_statement(c: &Cst, n: NodeId) -> Option<NodeId> {
    let mut s = n;
    loop {
        match c.kind(s) {
            ParBlock => return Some(s),
            Body | SeqBlock => {
                let items: Vec<NodeId> = list(c, s).into_iter().filter(|&x| c.kind(x) != EmptyStmt).collect();
                if items.len() != 1 {
                    return None;
                }
                s = items[0];
            }
            _ => return Some(s),
        }
    }
}

fn override_only_calls_super(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    for i in classes.user() {
        let cls = &classes.list[i];
        for m in &cls.methods {
            if m.name == "new" {
                continue;
            }
            let Some(body) = m.body else { continue };
            let Some(st) = sole_statement(c, body) else { continue };
            let call = match c.kind(st) {
                ExprStmt => exprs(c, st).into_iter().next(),
                ReturnStmt => exprs(c, st).into_iter().next(),
                _ => None,
            };
            let Some(call) = call.map(|x| strip_parens(c, x)) else {
                continue;
            };
            if c.kind(call) != CallExpr || call_name(c, call) != Some(m.name.as_str()) {
                continue;
            }
            let callee = c.tree.children(call).next();
            if !callee.is_some_and(|x| c.kind(x) == MemberExpr && c.tree.bare_text(x).starts_with("super.")) {
                continue;
            }
            // `super.m` must bind to a base-class method (not in a class
            // whose base is unknown, e.g. uvm_pkg missing).
            if !classes.ancestor_has(i, &m.name) {
                continue;
            }
            let actual = call_args(c, call);
            if actual.len() != m.ports.len() {
                continue;
            }
            let same = actual.iter().zip(&m.ports).all(|(a, p)| {
                a.is_some_and(|a| {
                    let a = strip_parens(c, a);
                    let a = assign_expr(c, a).map(|(l, _, _)| l).unwrap_or(a);
                    ident(c, a) == Some(p.name.as_str())
                })
            });
            if !same {
                continue;
            }
            s.add(
                "SV-DSN-006",
                "OverrideOnlyCallsSuper",
                Severity::Info,
                format!(
                    "'{}::{}' only calls super.{}() with the same arguments. The override adds nothing; delete it.",
                    cls.name, m.name, m.name
                ),
                c.loc(m.tok),
            );
        }
    }
}

// ---- SV-DSN-007 ----

fn empty_override(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    for i in classes.user() {
        let cls = &classes.list[i];
        for m in &cls.methods {
            let Some((k, Ok(bi))) = classes.override_of(i, &m.name) else {
                continue;
            };
            if classes.list[k].library {
                continue;
            }
            let Some(body) = m.body else { continue };
            if count_statements(c, body) != 0 {
                continue;
            }
            let base = &classes.list[k].methods[bi];
            if base.body.map(|b| count_statements(c, b)).unwrap_or(0) == 0 {
                continue;
            }
            s.add(
                "SV-DSN-007",
                "EmptyOverrideOfConcrete",
                Severity::Warning,
                format!(
                    "'{}::{}' is empty and replaces '{}::{}', which does work. Subclasses that switch inherited \
                     behavior off break code that relies on the base contract; add a config knob \
                     in the base class instead.",
                    cls.name, m.name, classes.list[k].name, base.name
                ),
                c.loc(m.tok),
            );
        }
    }
}

// ---- SV-DSN-008 ----

fn mutable_package_state(c: &Cst, s: &mut Sink) {
    for p in c.nodes(PackageDecl) {
        let name = c.tree.child(p, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default();
        if name == "std" || name == "uvm_pkg" {
            continue;
        }
        if c.first_tok(p).is_some_and(|t| {
            let f = file_of(c, t);
            f.contains("uvm_mock.svh") || f.contains("uvm_pkg.sv")
        }) {
            continue;
        }
        for item in c.tree.children(p).filter(|&x| c.kind(x) == DataDecl) {
            if c.toks(item)
                .any(|t| matches!(c.text_of(t), "const" | "localparam" | "parameter"))
            {
                continue;
            }
            // Nets and instances are not variables.
            if is_net_or_instance(c, item) {
                continue;
            }
            for (var, tok) in decl_names(c, item) {
                s.add(
                    "SV-DSN-008",
                    "MutablePackageState",
                    Severity::Warning,
                    format!(
                        "Package variable '{name}::{var}' is global mutable state shared by every env instance and test in the run. \
                         Move it into a config object or component, or make it const."
                    ),
                    c.loc(tok),
                );
            }
        }
    }
}

fn is_net_or_instance(c: &Cst, d: NodeId) -> bool {
    let nets = [
        "wire",
        "tri",
        "wand",
        "wor",
        "tri0",
        "tri1",
        "supply0",
        "supply1",
        "triand",
        "trior",
        "trireg",
        "uwire",
        "interconnect",
    ];
    let first = c.first_tok(d).map(|t| c.text_of(t)).unwrap_or("");
    nets.contains(&first)
        || c.tree
            .child(d, DataType)
            .and_then(|t| c.first_tok(t))
            .is_some_and(|t| nets.contains(&c.text_of(t)))
}

// ---- SV-DSN-010 ----

fn query_changes_state(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    for i in classes.user() {
        let cls = &classes.list[i];
        for m in &cls.methods {
            let n = m.name.as_str();
            if m.is_task {
                continue;
            }
            if !(n.starts_with("get_") || n.starts_with("is_") || n.starts_with("has_")) || n.starts_with("get_next_") {
                continue;
            }
            let Some(body) = m.body else { continue };
            let mut changed: Option<String> = None;
            let g = crate::g_design_types::globals(c);
            let env = crate::g_design_types::Env::new(c, &classes, Some(i), Some(m.decl), &g);
            for w in written_names(c, &env, body, true) {
                if local_or_port(c, m.decl, &w) {
                    continue;
                }
                if property_owner(&classes, i, &w).is_none() {
                    continue;
                }
                if changed.as_ref().map_or(true, |x| w < *x) {
                    changed = Some(w);
                }
            }
            if let Some(sym) = changed {
                s.add(
                    "SV-DSN-010",
                    "QueryChangesState",
                    Severity::Info,
                    format!(
                        "'{}::{n}' reads like a query but changes '{sym}'. Callers, checks and assertions \
                         that call it twice see different results; split the update into its own \
                         method or rename it.",
                        cls.name
                    ),
                    c.loc(m.tok),
                );
            }
        }
    }
}

/// `swe::rootSymbol`'s name: the variable at the root of `q[i]`, `s.f`.
pub(crate) fn root_name<'a>(c: &'a Cst, e: NodeId) -> Option<&'a str> {
    let mut e = strip_parens(c, e);
    for _ in 0..64 {
        match c.kind(e) {
            NameExpr => return ident(c, e),
            SelectExpr | MemberExpr => e = strip_parens(c, c.tree.children(e).next()?),
            _ => return None,
        }
    }
    None
}

/// `swe::writtenSymbols` by name (roots of assigned, incremented and
/// mutated expressions). Output arguments of user routines are not
/// resolved here.
pub(crate) fn written_names(c: &Cst, env: &crate::g_design_types::Env, body: NodeId, container: bool) -> Vec<String> {
    let mut out = Vec::new();
    let root = |e: NodeId, out: &mut Vec<String>| out.extend(root_name(c, e).map(str::to_string));
    for x in subtree(c, body) {
        match c.kind(x) {
            BinaryExpr => {
                if let Some((l, _, _)) = assign_expr(c, x) {
                    // `<=` is an assignment only as a statement.
                    if op(c, x) == "<=" && !c.parent(x).is_some_and(|p| c.kind(p) == ExprStmt) {
                        continue;
                    }
                    root(l, &mut out);
                }
            }
            PrefixExpr | PostfixExpr => {
                if matches!(op(c, x), "++" | "--") {
                    if let Some(e) = c.tree.children(x).next() {
                        root(e, &mut out);
                    }
                }
            }
            CallExpr => {
                let name = call_name(c, x).unwrap_or("");
                let Some(callee) = c.tree.children(x).next() else {
                    continue;
                };
                let args: Vec<Option<NodeId>> = call_args(c, x);
                match c.kind(callee) {
                    NameExpr if name.starts_with('$') => {
                        if matches!(
                            name,
                            "$cast" | "$sscanf" | "$fscanf" | "$fgets" | "$fread" | "$value$plusargs"
                        ) {
                            for a in args.iter().flatten() {
                                root(*a, &mut out);
                            }
                        }
                    }
                    ScopedExpr if c.tree.bare_text(callee) == "std::randomize" => {
                        for a in args.iter().flatten() {
                            root(*a, &mut out);
                        }
                    }
                    _ => {
                        if let Some((k, m)) = env.callee(x) {
                            // A user routine: its output and ref arguments.
                            let formals = &env.classes.list[k].methods[m].ports;
                            for (f, a) in formals.iter().zip(&args) {
                                if f.dir == "input" || (f.dir == "ref" && f.is_const) {
                                    continue;
                                }
                                if let Some(a) = a {
                                    root(*a, &mut out);
                                }
                            }
                        } else if c.kind(callee) == MemberExpr {
                            let recv = c.tree.children(callee).next();
                            let on_class = recv
                                .and_then(|r| env.expr_type(r))
                                .is_some_and(|t| !t.array && t.class.is_some());
                            let container_op = container
                                && !on_class
                                && matches!(
                                    name,
                                    "push_back"
                                        | "push_front"
                                        | "pop_back"
                                        | "pop_front"
                                        | "insert"
                                        | "delete"
                                        | "sort"
                                        | "rsort"
                                        | "reverse"
                                        | "shuffle"
                                );
                            if name == "randomize" || name == "srandom" || container_op {
                                if let Some(r) = recv {
                                    root(r, &mut out);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

// ---- SV-DSN-012 ----

fn primitive_obsession(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    for i in classes.user() {
        let role = classes.role(i);
        if !matches!(role, Role::Item | Role::Config | Role::Object) {
            continue;
        }
        let cls = &classes.list[i];
        let mut reported: HashSet<String> = HashSet::new();
        for cd in c.tree.children(cls.node).filter(|&x| c.kind(x) == ConstraintDecl) {
            let Some(block) = c.tree.child(cd, ConstraintBlock) else {
                continue;
            };
            for item in c.tree.children(block).filter(|&x| c.kind(x) == ConstraintItem) {
                let Some(e) = c.tree.children(item).next() else {
                    continue;
                };
                if c.tree.children(item).count() != 1 {
                    continue;
                }
                let e = strip_parens(c, e);
                if c.kind(e) != InsideExpr {
                    continue;
                }
                let mut parts = c.tree.children(e);
                let (Some(l), Some(rl)) = (parts.next(), parts.next()) else {
                    continue;
                };
                let Some(field) = referenced_name(c, l) else { continue };
                let Some(p) = cls.props.iter().find(|p| p.name == field) else {
                    continue;
                };
                if !p.rand {
                    continue;
                }
                let Some(ty) = p.ty else { continue };
                if !multi_bit_integral(c, ty) {
                    continue;
                }
                let items: Vec<NodeId> = c.tree.children(rl).collect();
                if items.len() < 3 || items.len() > 16 {
                    continue;
                }
                if items.iter().any(|&x| c.kind(x) == OpenRange || !constant_expr(c, x)) {
                    continue;
                }
                if !reported.insert(field.to_string()) {
                    continue;
                }
                s.add(
                    "SV-DSN-012",
                    "PrimitiveObsession",
                    Severity::Info,
                    format!(
                        "'{field}' only ever takes {} listed values, so it is an enumeration written as a bit vector. Declare a typedef \
                         enum: logs and waveforms then show names instead of numbers, and a wrong value no \
                         longer compiles.",
                        items.len()
                    ),
                    c.loc(p.tok),
                );
            }
        }
    }
}

/// An integral, non-enum type wider than one bit, as written.
fn multi_bit_integral(c: &Cst, ty: NodeId) -> bool {
    let Some(first) = c.first_tok(ty).map(|t| c.text_of(t)) else {
        return false;
    };
    match first {
        "int" | "integer" | "shortint" | "longint" | "byte" => true,
        "bit" | "logic" | "reg" => c
            .tree
            .children(ty)
            .any(|d| c.kind(d) == Dim && dim_wider_than_one(c, d)),
        _ => false,
    }
}

fn dim_wider_than_one(c: &Cst, d: NodeId) -> bool {
    let v: Vec<NodeId> = c.tree.children(d).collect();
    if v.len() != 2 {
        return true;
    }
    let (a, b) = (c.trimmed(v[0]), c.trimmed(v[1]));
    a != b
}

/// `swe::isConstantExpr` on the syntax: literals and operators of them.
fn constant_expr(c: &Cst, e: NodeId) -> bool {
    let e = strip_parens(c, e);
    match c.kind(e) {
        Literal => true,
        BinaryExpr => c.tree.children(e).all(|x| constant_expr(c, x)),
        PrefixExpr => c.tree.children(e).all(|x| constant_expr(c, x)),
        _ => false,
    }
}

// ---- SV-DSN-013 ----

fn fatal_stub_override(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    for i in classes.user() {
        let cls = &classes.list[i];
        for m in &cls.methods {
            let Some((k, Ok(bi))) = classes.override_of(i, &m.name) else {
                continue;
            };
            if classes.list[k].library {
                continue;
            }
            let Some(body) = m.body else { continue };
            let level = strongest_report(c, body);
            if level != Level::Error && level != Level::Fatal {
                continue;
            }
            if !only_reports(c, body) {
                continue;
            }
            s.add(
                "SV-DSN-013",
                "FatalStubOverride",
                Severity::Warning,
                format!(
                    "'{}::{}' only reports an error, refusing '{}::{}'. Anything holding a base handle now fails at run \
                     time. Move the shared part into a common base and give each variant only \
                     what it supports.",
                    cls.name, m.name, classes.list[k].name, classes.list[k].methods[bi].name
                ),
                c.loc(m.tok),
            );
        }
    }
}

fn only_reports(c: &Cst, body: NodeId) -> bool {
    fn ok(c: &Cst, n: NodeId) -> bool {
        match stmt(c, n) {
            Some(St::Expr) => {
                if let Some(e) = exprs(c, n).into_iter().next().map(|e| strip_parens(c, e)) {
                    if c.kind(e) == CallExpr
                        && (report_level(c, e) != Level::None || call_name(c, e).is_some_and(|x| x.starts_with('$')))
                    {
                        return true;
                    }
                }
                is_report_scaffolding(c, n)
            }
            Some(St::If) => is_report_guard(c, n) && nested_stmts(c, n).into_iter().all(|x| ok(c, x)),
            Some(St::Decl) => is_report_scaffolding(c, n),
            Some(St::Return | St::Timed) => false,
            Some(St::Seq | St::Par(_)) => list(c, n).into_iter().all(|x| ok(c, x)),
            Some(_) => nested_stmts(c, n).into_iter().all(|x| ok(c, x)),
            None => true,
        }
    }
    list(c, body).into_iter().all(|x| ok(c, x))
}

// ---- UVM-ROL-007 ----

fn procedural_timeout(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    for i in classes.user() {
        if classes.role(i) != Role::Monitor {
            continue;
        }
        let cls = &classes.list[i];
        for m in cls.methods.iter().filter(|m| !m.out_of_block) {
            let Some(body) = m.body else { continue };
            for b in all_stmts(c, body) {
                if stmt(c, b) != Some(St::Par(Join::Any)) || !has_counting_branch(c, b) {
                    continue;
                }
                s.add(
                    "UVM-ROL-007",
                    "ProceduralTimeoutInMonitor",
                    Severity::Info,
                    format!(
                        "Monitor '{}' times a handshake by counting cycles in a \
                         fork ... join_any. Write the rule as an assertion in the interface (req |-> \
                         ##[1:N] ack) and keep the monitor to collecting transactions.",
                        cls.name
                    ),
                    start(c, b),
                );
            }
        }
    }
}

fn has_counting_branch(c: &Cst, body: NodeId) -> bool {
    let stmts = all_stmts(c, body);
    let repeat = stmts.iter().any(|&r| {
        stmt(c, r) == Some(St::Repeat)
            && body_of(c, r).is_some_and(|b| {
                all_stmts(c, b).into_iter().any(|t| {
                    stmt(c, t) == Some(St::Timed)
                        && c.tree
                            .child(t, TimingControl)
                            .and_then(|tc| c.keyword(tc))
                            .is_some_and(|k| c.text_of(k) == "@")
                })
            })
    });
    let level = strongest_report(c, body);
    repeat && (level == Level::Error || level == Level::Fatal)
}

// ---- UVM-CFG-006 ----

fn plusargs_outside_test(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    for i in classes.user() {
        let role = classes.role(i);
        if role == Role::Test || role == Role::Config {
            continue;
        }
        let cls = &classes.list[i];
        let g = crate::g_design_types::globals(c);
        for m in &cls.methods {
            let Some(body) = m.body else { continue };
            let env = crate::g_design_types::Env::new(c, &classes, Some(i), Some(m.decl), &g);
            for call in calls(c, body) {
                let name = call_name(c, call).unwrap_or("");
                let read = match name {
                    "$test$plusargs" | "$value$plusargs" => true,
                    "get_arg_value" | "get_arg_values" | "get_arg_matches" | "get_plusargs" => {
                        env.callee(call).is_some_and(|(k, _)| classes.list[k].library)
                    }
                    _ => false,
                };
                if !read {
                    continue;
                }
                s.add(
                    "UVM-CFG-006",
                    "PlusargsOutsideTest",
                    Severity::Warning,
                    format!(
                        "{} '{}' reads the command line with {name}() in {}(). Hidden switches make runs hard to \
                         reproduce and the class hard to reuse; read plusargs once in the test or a \
                         config object and pass the values down through the config.",
                        role.name(),
                        cls.name,
                        m.name
                    ),
                    start(c, call),
                );
            }
        }
    }
}

// ---- UVM-ARC-004 ----

fn string_hierarchy_lookup(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    for i in classes.user() {
        let role = classes.role(i);
        if role == Role::Test {
            continue;
        }
        let cls = &classes.list[i];
        let g = crate::g_design_types::globals(c);
        for m in &cls.methods {
            if m.name == "end_of_elaboration_phase" {
                continue;
            }
            let Some(body) = m.body else { continue };
            let env = crate::g_design_types::Env::new(c, &classes, Some(i), Some(m.decl), &g);
            for call in calls(c, body) {
                let name = call_name(c, call).unwrap_or("");
                if !matches!(name, "find" | "find_all" | "lookup") {
                    continue;
                }
                if !env.callee(call).is_some_and(|(k, _)| classes.list[k].library) {
                    continue;
                }
                s.add(
                    "UVM-ARC-004",
                    "StringHierarchyLookup",
                    Severity::Warning,
                    format!(
                        "{} '{}' looks up components by path with '{}' in {}(). The dependency is invisible and the lookup returns \
                         null as soon as the hierarchy is renamed. Get the handle through config_db, \
                         a config object or a TLM connection.",
                        role.name(),
                        cls.name,
                        normalized(c, call),
                        m.name
                    ),
                    start(c, call),
                );
            }
        }
    }
}

/// `swe::normalizedText`: tokens joined, any trivia becoming one space.
pub(crate) fn normalized(c: &Cst, n: NodeId) -> String {
    let mut out = String::new();
    for t in c.tree.token_range(n) {
        let tok = &c.tree.tokens[t as usize];
        if !out.is_empty() && !c.tree.trivia(t).is_empty() && !tok.from_macro {
            out.push(' ');
        }
        out.push_str(&tok.text);
    }
    out
}

// ---- UVM-ARC-003 ----

fn file_in_many_packages(c: &Cst, s: &mut Sink) {
    // file -> [(package name, package name token, class name)]
    let mut by_file: BTreeMap<String, Vec<(String, u32, String)>> = BTreeMap::new();
    for p in c.nodes(PackageDecl) {
        let Some(nm) = c.tree.child(p, Name) else { continue };
        let Some(ptok) = c.first_tok(nm) else { continue };
        let pname = c.text_of(ptok).to_string();
        let pfile = file_of(c, ptok);
        if pname == "std" || pname == "uvm_pkg" || pfile.contains("uvm_mock.svh") || pfile.contains("uvm_pkg.sv") {
            continue;
        }
        for k in c.tree.children(p).filter(|&x| c.kind(x) == ClassDecl) {
            let Some(knm) = c.tree.child(k, Name) else { continue };
            let Some(ktok) = c.first_tok(knm) else { continue };
            let kfile = file_of(c, ktok);
            if kfile.is_empty() || kfile == pfile {
                continue;
            }
            let sites = by_file.entry(kfile).or_default();
            if !sites.iter().any(|(n, _, _)| *n == pname) {
                sites.push((pname.clone(), ptok, c.text_of(ktok).to_string()));
            }
        }
    }
    for sites in by_file.values() {
        if sites.len() < 2 {
            continue;
        }
        let pkgs = sites.iter().map(|(n, _, _)| n.as_str()).collect::<Vec<_>>().join(", ");
        for (_, ptok, cls) in &sites[1..] {
            s.add(
                "UVM-ARC-003",
                "FileIncludedInMultiplePackages",
                Severity::Error,
                format!(
                    "'{cls}' is compiled into packages {pkgs} because its file is `include'd into each. That creates distinct types with the \
                     same name, so $cast, config_db and TLM connections between them fail at run time. \
                     Include it in one package and import that package elsewhere."
                ),
                c.loc(*ptok),
            );
        }
    }
}
