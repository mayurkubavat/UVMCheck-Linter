//! A class index over one syntax tree, standing in for slang's class symbols
//! (`swe::userClasses`, `classMethods`, `classRole`, `getOverride`), and the
//! report-call helpers the class rules share.

use std::collections::HashMap;

use sv_syntax::{Child, NodeId, SyntaxKind};

use crate::support::statements::*;

use crate::syntax::Cst;

use SyntaxKind::*;

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
    pub(crate) by_name: HashMap<String, Vec<usize>>,
    pub(crate) typedefs: HashMap<String, String>,
    pub(crate) by_node: HashMap<u32, usize>,
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

pub(crate) fn name_says_config(n: &str) -> bool {
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

pub(crate) fn decl_names(c: &Cst, n: NodeId) -> Vec<(String, u32)> {
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
        let new_dir = toks.iter().find(|t| matches!(**t, "input" | "output" | "inout" | "ref")).copied();
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
            out.push(Port { name, ty, dir, is_const });
        }
    }
    out
}

pub(crate) fn method_name(c: &Cst, decl: NodeId) -> Option<(String, u32, Option<String>)> {
    let nm = c.tree.child(decl, Name)?;
    let toks: Vec<u32> = c.toks(nm).collect();
    let last = *toks.last()?;
    let scope = (toks.len() >= 3 && c.text_of(toks[toks.len() - 2]) == "::").then(|| c.text_of(toks[0]).to_string());
    Some((c.text_of(last).to_string(), last, scope))
}

pub(crate) fn has_tok(c: &Cst, n: NodeId, s: &str) -> bool {
    c.toks(n).any(|t| c.text_of(t) == s)
}

pub(crate) fn meth(c: &Cst, decl: NodeId, virtual_kw: bool, out_of_block: bool) -> Option<Meth> {
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

    pub(crate) fn build(c: &Cst) -> Classes {
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
                .and_then(|h| c.tree.children(h).find(|&e| c.kind(e) == Extends && has_tok(c, e, "extends")))
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
            list.push(Class { node: n, name, tok, base, generic, library, props, methods, pure });
        }
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        let mut by_node = HashMap::new();
        for (i, k) in list.iter().enumerate() {
            by_name.entry(k.name.clone()).or_default().push(i);
            by_node.insert(k.node.0, i);
        }
        Classes { list, by_name, typedefs, by_node }
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
        self.chain(i).into_iter().map(|(n, _)| n).find(|n| n.starts_with("uvm_"))
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
    pub(crate) fn is_virtual(&self, i: usize, name: &str) -> bool {
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
    calls(c, n).into_iter().map(|x| report_level(c, x)).max().unwrap_or(Level::None)
}

pub(crate) fn is_report_query(n: &str) -> bool {
    matches!(n, "uvm_report_enabled" | "get_report_verbosity_level" | "get_report_action" | "uvm_get_report_object")
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

pub(crate) fn is_report_local(name: &str) -> bool {
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

pub(crate) fn list_or_self(c: &Cst, n: NodeId) -> Vec<NodeId> {
    if c.kind(n) == Body {
        list(c, n)
    } else {
        vec![n]
    }
}

pub(crate) fn count_one(c: &Cst, n: NodeId) -> usize {
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
pub(crate) fn written_names(
    c: &Cst,
    env: &crate::support::class_metrics::Env,
    body: NodeId,
    container: bool,
) -> Vec<String> {
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
                        if matches!(name, "$cast" | "$sscanf" | "$fscanf" | "$fgets" | "$fread" | "$value$plusargs") {
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
                            let on_class =
                                recv.and_then(|r| env.expr_type(r)).is_some_and(|t| !t.array && t.class.is_some());
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
