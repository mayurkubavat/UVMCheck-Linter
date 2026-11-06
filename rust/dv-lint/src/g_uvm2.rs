//! UVM and RAL rules that read the class model and method bodies together
//! (builder slice N6.3, group B), ported from the C++ rules that walk
//! slang's AST. Bodies come from the sv-syntax trees; the class model gives
//! each class's base chain and members.
//!
//! slang's AST visits each statement and expression once per elaborated
//! context; here a class's body is read once (its default specialization).
//! An expression slang cannot bind drops the statement list around it from
//! the AST, which syntax alone cannot see, so these ports assume clean code.

use std::collections::{HashMap, HashSet};

use dv_model::{Class, ClassId, Design, Loc};
use sv_syntax::{Child, NodeId, SyntaxKind};

use crate::g_uvm::{
    arg_list, assignment, body_items, callee, class_routines, decl_name, enclosing_class,
    name_node, ordered_expr, params, routine_last_name, routine_text, stmt_expr, subtree, trim_ws,
};
use crate::syntax::Cst;
use crate::Check::{Mixed, Model};
use crate::{Rule, Severity, Sink};
use SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "UVM-PHS-005", name: "SuperBuildPhaseOrder", check: Mixed(super_build_phase_order) },
        Rule { id: "UVM-SEQ-005", name: "EmptySequenceBody", check: Mixed(empty_sequence_body) },
        Rule { id: "UVM-SEQ-009", name: "SequenceBodyWithoutStartItem", check: Mixed(body_without_start_item) },
        Rule { id: "UVM-SEQ-010", name: "SequenceItemCreateWithoutType", check: Mixed(item_create_without_type) },
        Rule { id: "UVM-STY-003", name: "ComponentNameMismatch", check: Mixed(component_name_mismatch) },
        Rule { id: "UVM-FAC-004", name: "ParameterizedClassFactoryRegistration", check: Mixed(param_class_factory) },
        Rule { id: "UVM-OBJ-002", name: "UvmObjectionInLoop", check: Mixed(objection_in_loop) },
        Rule { id: "UVM-OBJ-003", name: "ObjectionControlFlowMismatch", check: Mixed(objection_control_flow) },
        Rule { id: "UVM-OBJ-004", name: "OrphanSubsequenceObjectionHang", check: Mixed(orphan_objection) },
        Rule { id: "UVM-OBS-002", name: "BlockingInObserver", check: Mixed(blocking_in_observer) },
        Rule { id: "UVM-SCB-001", name: "ScoreboardSilentDrop", check: Mixed(scoreboard_silent_drop) },
        Rule { id: "UVM-SCB-002", name: "ScoreboardLeftoverQueue", check: Mixed(scoreboard_leftover_queue) },
        Rule { id: "UVM-TLM-004", name: "TlmTopologyChecker", check: Mixed(tlm_topology) },
        Rule { id: "UVM-TLM-005", name: "AnalysisTransactionMutation", check: Mixed(analysis_mutation) },
        Rule { id: "UVM-TLM-006", name: "AnalysisFifoDrainStarvation", check: Mixed(fifo_drain_starvation) },
        Rule { id: "UVM-PHS-010", name: "BuildPhaseCrossComponentAccess", check: Mixed(build_phase_cross_access) },
        Rule { id: "UVM-RAL-001", name: "DirectRalMirrorTampering", check: Mixed(ral_mirror_tampering) },
        Rule { id: "UVM-REP-005", name: "LowVerbosityDebugReport", check: Mixed(low_verbosity_debug) },
        Rule { id: "UVM-LOG-002", name: "ProhibitedDisplayInUvm", check: Mixed(display_in_uvm) },
        Rule { id: "UVM-SEQ-011", name: "UncheckedPSequencerCast", check: Mixed(unchecked_p_sequencer_cast) },
        Rule { id: "UVM-OBJ-005", name: "ObjectionWithoutDescription", check: Mixed(objection_without_description) },
        Rule { id: "UVM-FAC-005", name: "GlobalFactoryOverridePollution", check: Model(global_factory_override) },
        Rule { id: "UVM-DAG-001", name: "UvmDanglingConfigString", check: Mixed(dangling_config_string) },
        Rule { id: "RAL-ACC-001", name: "DirectMirrorSetWithoutWrite", check: Mixed(direct_mirror_set) },
        Rule { id: "RAL-BLK-001", name: "RegBlockNeverLocked", check: Mixed(reg_block_never_locked) },
        Rule { id: "RAL-MAP-002", name: "RalMapAddressIntervalOverlap", check: Mixed(ral_map_overlap) },
        Rule { id: "UVM-SEQ-014", name: "UnusedSequence", check: Mixed(unused_sequence) },
        Rule { id: "UVM-SEQ-015", name: "UnusedSequenceItem", check: Mixed(unused_sequence_item) },
    ]
}

// ---- the design and its trees together ----

type Key = (u32, u32, u32, Option<(u32, u32)>);

fn key(l: Loc) -> Key {
    (l.file, l.line, l.column, l.expansion)
}

/// A node of one of the trees.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct At {
    tree: usize,
    node: NodeId,
}

/// Design classes and their declarations in the trees.
struct Ix<'a, 't> {
    d: &'a Design,
    csts: &'a [Cst<'t>],
    decl: HashMap<ClassId, At>,
    class_at: HashMap<At, ClassId>,
    /// Out-of-block bodies by (class name, method name).
    out_of_block: HashMap<(String, String), At>,
}

impl<'a, 't> Ix<'a, 't> {
    fn new(d: &'a Design, csts: &'a [Cst<'t>]) -> Self {
        let by_loc: HashMap<Key, ClassId> = d.classes().iter().map(|c| (key(c.loc), c.id)).collect();
        let mut decl = HashMap::new();
        let mut class_at = HashMap::new();
        let mut out_of_block = HashMap::new();
        for (ti, c) in csts.iter().enumerate() {
            for n in c.nodes(ClassDecl) {
                let Some(t) = name_node(c, n).and_then(|x| c.first_tok(x)) else { continue };
                if let Some(&id) = by_loc.get(&key(c.loc(t))) {
                    let at = At { tree: ti, node: n };
                    decl.entry(id).or_insert(at);
                    class_at.insert(at, id);
                }
            }
            for n in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)) {
                if c.parent(n).is_some_and(|p| c.kind(p) == ClassDecl) {
                    continue;
                }
                let Some(nm) = name_node(c, n) else { continue };
                let toks = c.tree.token_range(nm);
                if toks.len() >= 3 && c.text_of(toks[toks.len() - 2]) == "::" {
                    let cls = c.text_of(toks[toks.len() - 3]).to_string();
                    let m = c.text_of(toks[toks.len() - 1]).to_string();
                    out_of_block.entry((cls, m)).or_insert(At { tree: ti, node: n });
                }
            }
        }
        Ix { d, csts, decl, class_at, out_of_block }
    }

    fn cst(&self, at: At) -> &'a Cst<'t> {
        &self.csts[at.tree]
    }

    /// The design class a class declaration stands for.
    fn class_of(&self, at: At) -> Option<&'a Class> {
        self.class_at.get(&at).map(|&id| self.d.class(id))
    }

    /// slang's `cls->members()` subroutines with bodies: methods defined in
    /// the class (not extern ones).
    fn own_routines(&self, id: ClassId) -> Vec<At> {
        let Some(&at) = self.decl.get(&id) else { return Vec::new() };
        class_routines(self.cst(at), at.node).into_iter().map(|node| At { tree: at.tree, node }).collect()
    }

    /// `swe::classMethods`: [`Self::own_routines`] plus extern bodies.
    fn all_routines(&self, id: ClassId) -> Vec<At> {
        let mut v = self.own_routines(id);
        let Some(&at) = self.decl.get(&id) else { return v };
        let c = self.cst(at);
        let cls = self.d.class(id).name.clone();
        for p in c.tree.children(at.node).filter(|&n| c.kind(n) == MethodPrototype) {
            if c.tok(p, "pure").is_some() {
                continue;
            }
            let m = routine_last_name(c, p);
            if let Some(&b) = self.out_of_block.get(&(cls.clone(), m)) {
                v.push(b);
            }
        }
        v
    }

    /// The class a routine belongs to: its enclosing declaration, or the
    /// class an out-of-block body names.
    fn routine_class(&self, at: At) -> Option<ClassId> {
        let c = self.cst(at);
        if let Some(k) = c.parent(at.node).filter(|&p| c.kind(p) == ClassDecl) {
            return self.class_at.get(&At { tree: at.tree, node: k }).copied();
        }
        let nm = name_node(c, at.node)?;
        let toks = c.tree.token_range(nm);
        if toks.len() >= 3 && c.text_of(toks[toks.len() - 2]) == "::" {
            let cls = c.text_of(toks[toks.len() - 3]);
            return self.d.classes().iter().find(|k| k.name == cls && !k.is_parameterized).map(|k| k.id);
        }
        None
    }

    /// Classes slang's UvmModel indexes (no parameterized ones), in order.
    fn model_classes(&self) -> impl Iterator<Item = &'a Class> {
        self.d.classes().iter().filter(|c| !c.is_parameterized)
    }
}

/// `UvmModel::isDerivedFrom`: the class or a resolved base is named `name`.
fn derived(d: &Design, id: ClassId, name: &str) -> bool {
    d.class(id).name == name || d.ancestors(id).any(|a| a.name == name)
}

fn is_component(d: &Design, id: ClassId) -> bool {
    derived(d, id, "uvm_component")
}

fn is_sequence(d: &Design, id: ClassId) -> bool {
    derived(d, id, "uvm_sequence_base") || derived(d, id, "uvm_sequence")
}

/// `swe::Role`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Role {
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

fn role_name(r: Role) -> &'static str {
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

fn class_role(d: &Design, id: ClassId) -> Role {
    for a in d.ancestors(id).take(64) {
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
        match n {
            "uvm_agent" => return Role::Agent,
            "uvm_scoreboard" => return Role::Scoreboard,
            "uvm_subscriber" => return Role::Subscriber,
            "uvm_env" => return Role::Env,
            "uvm_test" => return Role::Test,
            _ => {}
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
        if matches!(n, "uvm_sequence" | "uvm_sequence_base" | "uvm_reg_sequence") {
            return Role::Sequence;
        }
        if matches!(n, "uvm_sequence_item" | "uvm_transaction") {
            return Role::Item;
        }
        if n == "uvm_component" || n.starts_with("uvm_tlm") || n == "uvm_root" {
            return Role::Component;
        }
        if n == "uvm_object" || n == "uvm_void" {
            let l = d.class(id).name.to_ascii_lowercase();
            return if l.contains("cfg") || l.contains("config") { Role::Config } else { Role::Object };
        }
    }
    Role::None
}

/// Name of the nearest uvm_* ancestor.
fn uvm_base_name(d: &Design, id: ClassId) -> &str {
    d.ancestors(id).take(64).find(|a| a.name.starts_with("uvm_")).map(|a| a.name.as_str()).unwrap_or("")
}

/// `swe::isLibrarySymbol` for a class.
fn is_library(d: &Design, c: &Class) -> bool {
    if c.name.starts_with("uvm_") || c.scope.split("::").any(|s| s == "uvm_pkg" || s.starts_with("uvm_")) {
        return true;
    }
    let f = d.file_name(c.loc);
    f.contains("uvm_mock.svh") || f.contains("uvm_pkg.sv")
}

/// `swe::userClasses`: model classes, then parameterized ones, minus the library.
fn user_classes(d: &Design) -> Vec<&Class> {
    let a = d.classes().iter().filter(|c| !c.is_parameterized);
    let b = d.classes().iter().filter(|c| c.is_parameterized);
    a.chain(b).filter(|c| !is_library(d, c)).collect()
}

/// A type as written, reduced to the class name it names.
fn type_head(s: &str) -> &str {
    let s = s.split(['#', '[', '(']).next().unwrap_or("");
    let s = s.rsplit("::").next().unwrap_or("");
    trim_ws(s)
}

fn class_named<'a>(d: &'a Design, name: &str) -> Option<&'a Class> {
    if name.is_empty() {
        return None;
    }
    d.classes().iter().find(|c| c.name == name && !c.is_parameterized).or_else(|| d.find_class(name))
}

// ---- statements ----

/// A case statement's items: each statement, and whether it is the default.
fn case_items(c: &Cst, n: NodeId) -> Vec<(NodeId, bool)> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut seen_selector = false;
    let mut expect = false;
    let mut default = false;
    for k in c.kids(n) {
        match *k {
            Child::Token(t) => match c.text_of(t) {
                "(" => depth += 1,
                ")" => {
                    depth -= 1;
                    if depth == 0 {
                        seen_selector = true;
                    }
                }
                ":" if seen_selector && depth == 0 => expect = true,
                "default" if seen_selector => {
                    expect = true;
                    default = true;
                }
                _ => {}
            },
            Child::Node(x) => {
                if matches!(c.kind(x), StmtLabel | Attribute) {
                    continue;
                }
                if expect {
                    out.push((x, default));
                    expect = false;
                    default = false;
                }
            }
        }
    }
    out
}

/// An if statement: condition, then-branch and else-branch.
fn if_parts(c: &Cst, n: NodeId) -> Option<(NodeId, NodeId, Option<NodeId>)> {
    let items: Vec<NodeId> = c.items(n);
    let cond = *items.first()?;
    let then = *items.get(1)?;
    let els = c.tree.child(n, ElseClause).and_then(|e| c.items(e).into_iter().next());
    Some((cond, then, els))
}

/// Statements a procedural block or routine holds in order.
fn block_stmts(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.items(n)
}

fn is_routine(k: SyntaxKind) -> bool {
    matches!(k, FunctionDecl | TaskDecl)
}

/// Calls in a subtree (slang's CallExpression), with `$display;` written
/// without parentheses.
fn calls_in(c: &Cst, n: NodeId) -> Vec<NodeId> {
    subtree(c, n)
        .into_iter()
        .filter(|&x| match c.kind(x) {
            CallExpr => true,
            NameExpr => {
                c.parent(x).is_some_and(|p| c.kind(p) == ExprStmt) && c.first_tok(x).is_some_and(|t| c.text_of(t).starts_with('$'))
            }
            _ => false,
        })
        .collect()
}

/// A call's name: `getSubroutineName()`.
fn call_name<'t>(c: &Cst<'t>, n: NodeId) -> &'t str {
    if c.kind(n) == NameExpr {
        return c.last_text(n);
    }
    callee(c, n)
}

/// The receiver of a method call (`a.b` in `a.b.f()`), when there is one.
fn receiver(c: &Cst, call: NodeId) -> Option<NodeId> {
    if c.kind(call) != CallExpr {
        return None;
    }
    let left = c.tree.children(call).next()?;
    (c.kind(left) == MemberExpr).then(|| c.tree.children(left).next()).flatten()
}

/// slang binds `a.b[0].c` as one name: only its outermost part keeps syntax.
fn is_name_chain(c: &Cst, n: NodeId) -> bool {
    match c.kind(n) {
        NameExpr | ScopedExpr => true,
        MemberExpr | SelectExpr => c.tree.children(n).next().is_some_and(|l| is_name_chain(c, l)),
        _ => false,
    }
}

/// The expression nodes of an argument list, as slang's `arguments()`
/// (written ones only).
fn args(c: &Cst, call: NodeId) -> Vec<Option<NodeId>> {
    let Some(list) = arg_list(c, call) else { return Vec::new() };
    params(c, list).into_iter().map(|a| a.and_then(|a| ordered_expr(c, a))).collect()
}

/// Strips parentheses (slang's conversions do not show in syntax).
fn unparen(c: &Cst, mut n: NodeId) -> NodeId {
    while c.kind(n) == ParenExpr {
        match c.items(n).into_iter().next() {
            Some(x) => n = x,
            None => break,
        }
    }
    n
}

/// The value of a string literal expression.
fn string_value(c: &Cst, n: NodeId) -> Option<String> {
    let n = unparen(c, n);
    if c.kind(n) != Literal {
        return None;
    }
    let toks = c.tree.token_range(n);
    if toks.len() != 1 {
        return None;
    }
    let t = c.text_of(toks[0]);
    let inner = t.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::new();
    let mut it = inner.chars();
    while let Some(ch) = it.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(x) => out.push(x),
            None => {}
        }
    }
    Some(out)
}

// ---- names and their class types ----

/// Declared names of a routine: ports and locals, with their types as written.
fn routine_vars(c: &Cst, f: NodeId) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    if let Some(pl) = c.tree.child(f, PortList) {
        let mut last = String::new();
        for p in c.tree.children(pl).filter(|&p| c.kind(p) == Port) {
            if let Some(dt) = c.tree.child(p, DataType) {
                last = c.tree.bare_text(dt);
            }
            for d in c.tree.children(p).filter(|&d| c.kind(d) == Declarator) {
                if let Some(nm) = name_node(c, d) {
                    vars.entry(c.tree.bare_text(nm)).or_insert_with(|| last.clone());
                }
            }
        }
    }
    for n in subtree(c, f) {
        if c.kind(n) != DataDecl {
            continue;
        }
        let Some(dt) = c.tree.child(n, DataType) else { continue };
        let ty = c.tree.bare_text(dt);
        for d in c.tree.children(n).filter(|&d| c.kind(d) == Declarator) {
            if let Some(nm) = name_node(c, d) {
                vars.entry(c.tree.bare_text(nm)).or_insert_with(|| ty.clone());
            }
        }
    }
    vars
}

/// Where an expression is evaluated: the routine's class and declared names.
struct Ctx {
    class: Option<ClassId>,
    vars: HashMap<String, String>,
}

impl Ix<'_, '_> {
    fn ctx(&self, at: At) -> Ctx {
        let c = self.cst(at);
        let mut r = at.node;
        loop {
            if is_routine(c.kind(r)) {
                break;
            }
            match c.parent(r) {
                Some(p) => r = p,
                None => {
                    return Ctx { class: None, vars: HashMap::new() };
                }
            }
        }
        let rat = At { tree: at.tree, node: r };
        Ctx { class: self.routine_class(rat), vars: routine_vars(c, r) }
    }

    /// A property's class type, in the class or its bases.
    fn property_class(&self, id: ClassId, name: &str) -> Option<ClassId> {
        let d = self.d;
        for c in std::iter::once(d.class(id)).chain(d.ancestors(id)) {
            if let Some(p) = c.properties.iter().find(|p| p.name == name) {
                return p.type_class.or_else(|| class_named(d, type_head(&p.ty)).map(|k| k.id));
            }
        }
        None
    }

    /// A method's return class type, in the class or its bases.
    fn method_class(&self, id: ClassId, name: &str) -> Option<ClassId> {
        let d = self.d;
        for c in std::iter::once(d.class(id)).chain(d.ancestors(id)) {
            if let Some(m) = c.methods.iter().find(|m| m.name == name) {
                return class_named(d, type_head(&m.return_type)).map(|k| k.id);
            }
        }
        None
    }

    /// The class type of an expression, as far as names tell.
    fn expr_class(&self, c: &Cst, cx: &Ctx, n: NodeId, depth: usize) -> Option<ClassId> {
        if depth > 16 {
            return None;
        }
        let n = unparen(c, n);
        match c.kind(n) {
            NameExpr => {
                let name = c.tree.bare_text(n);
                if name == "this" {
                    return cx.class;
                }
                if let Some(ty) = cx.vars.get(&name) {
                    return class_named(self.d, type_head(ty)).map(|k| k.id);
                }
                cx.class.and_then(|k| self.property_class(k, &name))
            }
            MemberExpr => {
                let left = c.tree.children(n).next()?;
                let member = c.last_text(n);
                if c.tree.bare_text(left) == "super" {
                    let base = self.d.class(cx.class?).base?;
                    return self.property_class(base, member);
                }
                let k = self.expr_class(c, cx, left, depth + 1)?;
                self.property_class(k, member)
            }
            CallExpr => {
                let left = c.tree.children(n).next()?;
                match c.kind(left) {
                    NameExpr => self.method_class(cx.class?, &c.tree.bare_text(left)),
                    MemberExpr => {
                        let recv = c.tree.children(left).next()?;
                        let k = if c.tree.bare_text(recv) == "super" {
                            self.d.class(cx.class?).base?
                        } else {
                            self.expr_class(c, cx, recv, depth + 1)?
                        };
                        self.method_class(k, c.last_text(left))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

// ---- syntax rules that ask the class model ----

const COMPONENT_BASES: [&str; 7] =
    ["uvm_component", "uvm_env", "uvm_agent", "uvm_driver", "uvm_monitor", "uvm_scoreboard", "uvm_test"];

/// The first model class with this name.
fn first_named<'a>(ix: &Ix<'a, '_>, name: &str) -> Option<&'a Class> {
    ix.model_classes().find(|c| c.name == name)
}

/// The component test of the class-walking C++ visitors.
fn comp_decl(ix: &Ix, c: &Cst, cls: NodeId) -> bool {
    let name = decl_name(c, cls);
    if name.is_empty() || name == "uvm_component" {
        return false;
    }
    if first_named(ix, &name).is_some_and(|k| is_component(ix.d, k.id)) {
        return true;
    }
    let base = crate::g_uvm::base_name_node(c, cls).map(|b| c.text(b)).unwrap_or_default();
    COMPONENT_BASES.iter().any(|b| base.contains(b))
}

/// The sequence test of the class-walking C++ visitors.
fn seq_decl(ix: &Ix, c: &Cst, cls: NodeId) -> bool {
    let name = decl_name(c, cls);
    if name.is_empty() || name == "uvm_sequence" || name == "uvm_sequence_base" {
        return false;
    }
    if first_named(ix, &name).is_some_and(|k| is_sequence(ix.d, k.id) || derived(ix.d, k.id, "uvm_sequence_base")) {
        return true;
    }
    crate::g_uvm::base_name_node(c, cls).is_some_and(|b| c.text(b).contains("uvm_sequence"))
}

/// Routines whose innermost class passes `test` (out-of-block ones have none).
fn routines_in<'c>(ix: &Ix, c: &'c Cst, test: fn(&Ix, &Cst, NodeId) -> bool) -> Vec<(String, NodeId)> {
    let mut memo: HashMap<NodeId, bool> = HashMap::new();
    let mut out = Vec::new();
    for f in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)) {
        let Some(cls) = enclosing_class(c, f) else { continue };
        let ok = *memo.entry(cls).or_insert_with(|| test(ix, c, cls));
        if ok {
            out.push((decl_name(c, cls), f));
        }
    }
    out
}

fn strip_comments_and_ws(input: &str) -> String {
    let b = input.as_bytes();
    let mut out = Vec::new();
    let (mut line, mut block) = (false, false);
    let mut i = 0;
    while i < b.len() {
        if line {
            if b[i] == b'\n' {
                line = false;
            }
        } else if block {
            if b[i] == b'*' && i + 1 < b.len() && b[i + 1] == b'/' {
                block = false;
                i += 1;
            }
        } else if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            line = true;
            i += 1;
        } else if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            block = true;
            i += 1;
        } else if !matches!(b[i], b' ' | b'\t' | b'\n' | b'\r') {
            out.push(b[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn super_build_phase_order(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for c in csts {
        for (cls, f) in routines_in(&ix, c, comp_decl) {
            if routine_last_name(c, f) != "build_phase" {
                continue;
            }
            let mut seen = false;
            for item in body_items(c, f) {
                if matches!(c.kind(item), DataDecl | EmptyStmt) {
                    continue;
                }
                if strip_comments_and_ws(&c.text(item)).contains("super.build_phase") {
                    if seen {
                        let Some(t) = c.first_tok(item) else { break };
                        s.add(
                            "UVM-PHS-005",
                            "SuperBuildPhaseOrder",
                            Severity::Warning,
                            format!(
                                "'super.build_phase(phase)' in class '{cls}' is called after preceding procedural statements. In UVM, build_phase executes top-down; \
                                 calling super.build_phase(phase) as the first statement ensures base-class configurations and sub-components are initialized before derived configuration."
                            ),
                            c.loc(t),
                        );
                    }
                    break;
                }
                seen = true;
            }
        }
    }
}

/// `task body` in a sequence class.
fn sequence_bodies(ix: &Ix, c: &Cst) -> Vec<(String, NodeId)> {
    routines_in(ix, c, seq_decl)
        .into_iter()
        .filter(|&(_, f)| c.kind(f) == TaskDecl && name_node(c, f).is_some_and(|n| c.trimmed(n) == "body"))
        .collect()
}

fn empty_sequence_body(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for c in csts {
        for (cls, f) in sequence_bodies(&ix, c) {
            if body_items(c, f).iter().any(|&i| c.kind(i) != EmptyStmt) {
                continue;
            }
            let Some(at) = crate::g_uvm::name_loc(c, f) else { continue };
            s.add(
                "UVM-SEQ-005",
                "EmptySequenceBody",
                Severity::Warning,
                format!(
                    "Sequence class '{cls}' contains an empty 'body()' task. Empty sequences produce no stimulus transactions and typically indicate incomplete VIP or obsolete test scaffolding."
                ),
                at,
            );
        }
    }
}

fn start_finish(c: &Cst, cls: &str, n: NodeId, started: &mut HashSet<String>, s: &mut Sink) {
    match c.kind(n) {
        ExprStmt => {
            let Some(e) = stmt_expr(c, n).filter(|&e| c.kind(e) == CallExpr) else { return };
            let Some(left) = c.tree.children(e).next() else { return };
            let method = match c.kind(left) {
                NameExpr => c.tree.bare_text(left),
                MemberExpr if !is_name_chain(c, left) => c.last_text(left).to_string(),
                _ => String::new(),
            };
            let arg = arg_list(c, e)
                .and_then(|l| params(c, l).first().copied().flatten())
                .map(|a| c.trimmed(a))
                .unwrap_or_else(|| "req".into());
            if method == "start_item" {
                started.insert(arg);
            } else if method == "finish_item" && !started.contains(&arg) {
                let Some(t) = c.first_tok(e) else { return };
                s.add(
                    "UVM-SEQ-009",
                    "SequenceBodyWithoutStartItem",
                    Severity::Error,
                    format!(
                        "Call to 'finish_item({arg})' in sequence '{cls}' body() is not preceded by a matching 'start_item({arg})'. \
                         In UVM sequence arbitration, start_item() requests sequencer grant and locks driver synchronization \
                         before finish_item() sends the transaction. Calling finish_item() without start_item() causes fatal sequencer errors."
                    ),
                    c.loc(t),
                );
            }
        }
        SeqBlock => {
            for i in block_stmts(c, n) {
                start_finish(c, cls, i, started, s);
            }
        }
        _ => {}
    }
}

fn body_without_start_item(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for c in csts {
        for (cls, f) in sequence_bodies(&ix, c) {
            let mut started = HashSet::new();
            for i in body_items(c, f) {
                start_finish(c, &cls, i, &mut started, s);
            }
        }
    }
}

/// slang's `NewClassExpression`: `new`, `new(...)`, `C::new(...)`.
fn is_new_class(c: &Cst, e: NodeId) -> bool {
    match c.kind(e) {
        NewExpr => {
            c.tok(e, "new").is_some()
                && c.tok(e, "[").is_none()
                && c.items(e).into_iter().all(|x| c.kind(x) == ArgList)
        }
        CallExpr => c
            .tree
            .children(e)
            .next()
            .is_some_and(|l| c.kind(l) == ScopedExpr && c.last_text(l) == "new"),
        _ => false,
    }
}

fn item_like(name: &str) -> bool {
    name == "req" || name == "rsp" || name.contains("item") || name.contains("tr") || name.contains("pkt")
}

fn item_create_without_type(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for c in csts {
        for (cls, f) in sequence_bodies(&ix, c) {
            for item in body_items(c, f) {
                for n in subtree(c, item) {
                    if let Some((_, lhs, rhs)) = stmt_expr(c, n).and_then(|e| assignment(c, e)) {
                        let l = c.trimmed(lhs);
                        if is_new_class(c, rhs) && item_like(&l) {
                            if let Some(t) = c.first_tok(rhs) {
                                s.add(
                                    "UVM-SEQ-010",
                                    "SequenceItemCreateWithoutType",
                                    Severity::Warning,
                                    format!(
                                        "Sequence item '{l}' in sequence '{cls}' body() is instantiated via direct 'new()'. \
                                         In UVM methodology (IEEE 1800.2 § 5.3), sequence items must be created through the factory \
                                         (e.g. '{l} = my_item::type_id::create(\"{l}\")' or 'create_item(...)') \
                                         to enable factory type overrides for error injection and stimulus extension."
                                    ),
                                    c.loc(t),
                                );
                            }
                        }
                    }
                    if c.kind(n) == DataDecl {
                        for dcl in c.tree.children(n).filter(|&x| c.kind(x) == Declarator) {
                            let Some(eq) = c.tok(dcl, "=") else { continue };
                            let Some(init) = c.node_after(dcl, eq) else { continue };
                            let var = name_node(c, dcl).map(|x| c.tree.bare_text(x)).unwrap_or_default();
                            if is_new_class(c, init) && item_like(&var) {
                                let Some(t) = c.first_tok(init) else { continue };
                                s.add(
                                    "UVM-SEQ-010",
                                    "SequenceItemCreateWithoutType",
                                    Severity::Warning,
                                    format!(
                                        "Sequence item '{var}' in sequence '{cls}' body() is initialized via direct 'new()'. \
                                         In UVM methodology (IEEE 1800.2 § 5.3), sequence items must be created through the factory \
                                         (e.g. 'type_id::create()') to enable factory overrides."
                                    ),
                                    c.loc(t),
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

fn component_name_mismatch(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for c in csts {
        for (cls, f) in routines_in(&ix, c, comp_decl) {
            if routine_last_name(c, f) != "new" {
                continue;
            }
            let mut param = "name".to_string();
            if let Some(pl) = c.tree.child(f, PortList) {
                if let Some(nm) = c
                    .tree
                    .children(pl)
                    .filter(|&p| c.kind(p) == Port)
                    .find_map(|p| c.tree.child(p, Declarator).and_then(|d| name_node(c, d)))
                {
                    param = c.tree.bare_text(nm);
                }
            }
            let mut list = None;
            for n in subtree(c, f) {
                let found = match c.kind(n) {
                    CallExpr => {
                        c.tree.children(n).next().is_some_and(|l| c.text(l).contains("super.new"))
                    }
                    NewExpr => false,
                    _ => false,
                };
                if found {
                    list = arg_list(c, n);
                }
            }
            let Some(list) = list else { continue };
            let Some(Some(a0)) = params(c, list).first().copied() else { continue };
            let Some(e) = ordered_expr(c, a0) else { continue };
            let text = c.trimmed(e);
            if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
                let Some(t) = c.first_tok(e) else { continue };
                s.add(
                    "UVM-STY-003",
                    "ComponentNameMismatch",
                    Severity::Error,
                    format!(
                        "Constructor of UVM component '{cls}' passes hardcoded string literal {text} to super.new(...) instead of forwarding parameter '{param}'. Hardcoded component names override factory instance names and break hierarchical config_db resolution."
                    ),
                    c.loc(t),
                );
            }
        }
    }
}

fn param_class_factory(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for (ti, c) in csts.iter().enumerate() {
        for n in c.nodes(ClassDecl) {
            if c.tree.child(n, Header).and_then(|h| c.tree.child(h, ParamList)).is_none() {
                continue;
            }
            let Some(ext) = crate::g_uvm::extends_clause(c, n) else { continue };
            let base = c.text(ext);
            if !base.contains("uvm_") {
                continue;
            }
            let comp = ["driver", "agent", "env", "component", "scoreboard"].iter().any(|k| base.contains(k));
            let Some(cls) = ix.class_of(At { tree: ti, node: n }) else { continue };
            for m in &cls.macros {
                let name = m.name.as_str();
                if !matches!(
                    name,
                    "uvm_component_utils" | "uvm_object_utils" | "uvm_component_utils_begin" | "uvm_object_utils_begin"
                ) {
                    continue;
                }
                let want = if comp { "`uvm_component_param_utils" } else { "`uvm_object_param_utils" };
                s.add(
                    "UVM-FAC-004",
                    "ParameterizedClassFactoryRegistration",
                    Severity::Error,
                    format!(
                        "Parameterized class '{}' declares standard non-parameterized macro '`{name}'. In UVM methodology (IEEE 1800.2 § 5.3), \
                         parameterized classes must use '{want}(...)' to ensure proper specialization and prevent \
                         factory type registry name collisions.",
                        cls.name
                    ),
                    m.loc,
                );
            }
        }
    }
}

// ---- objections ----

fn objection_in_loop(_d: &Design, csts: &[Cst], s: &mut Sink) {
    for c in csts {
        for n in calls_in(c, c.tree.root) {
            let name = call_name(c, n);
            if name != "raise_objection" && name != "drop_objection" {
                continue;
            }
            let mut in_loop = false;
            let mut cur = c.parent(n);
            while let Some(p) = cur {
                match c.kind(p) {
                    LoopStmt | DoWhileStmt => {
                        in_loop = true;
                        break;
                    }
                    FunctionDecl | TaskDecl | ClassDecl => break,
                    _ => {}
                }
                cur = c.parent(p);
            }
            if !in_loop {
                continue;
            }
            let Some(t) = c.first_tok(n) else { continue };
            s.add(
                "UVM-OBJ-002",
                "UvmObjectionInLoop",
                Severity::Warning,
                format!(
                    "UVM objection method '{name}' invoked inside a loop. Raising or dropping objections per iteration \
                     causes severe simulation performance degradation. Raise the objection once before the loop and drop it after the loop completes."
                ),
                c.loc(t),
            );
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Obj {
    Unheld,
    Held,
    Unknown,
}

fn obj_str(o: Obj) -> &'static str {
    match o {
        Obj::Unheld => "Unheld",
        Obj::Held => "Held",
        Obj::Unknown => "Unknown",
    }
}

struct Flow<'a, 'c> {
    c: &'a Cst<'c>,
    sub: String,
    out: Vec<(Severity, String, NodeId)>,
}

impl Flow<'_, '_> {
    fn add(&mut self, sev: Severity, msg: String, at: NodeId) {
        self.out.push((sev, msg, at));
    }

    /// `evaluateStatement`: returns whether the statement always returns,
    /// with the objection state after it.
    fn stmt(&mut self, n: NodeId, st: Obj) -> (bool, Obj) {
        let c = self.c;
        match c.kind(n) {
            SeqBlock | ParBlock | Body => self.list(&block_stmts(c, n), st),
            ExprStmt => {
                let text = stmt_expr(c, n).map(|e| c.text(e)).unwrap_or_default();
                let named = stmt_expr(c, n).map(|e| if c.kind(e) == CallExpr { callee(c, e) } else { "" }).unwrap_or("");
                if named == "raise_objection" || text.contains("raise_objection") {
                    if st == Obj::Held {
                        let msg = format!(
                            "Redundant 'raise_objection()' call in '{}': objection is already actively held along this control-flow path.",
                            self.sub
                        );
                        self.add(Severity::Warning, msg, n);
                    }
                    (false, Obj::Held)
                } else if named == "drop_objection" || text.contains("drop_objection") {
                    if st == Obj::Unheld {
                        let msg = format!(
                            "Premature 'drop_objection()' in '{}': no active objection was raised along this execution path. \
                             Dropping unraised objections causes UVM runtime fatal errors.",
                            self.sub
                        );
                        self.add(Severity::Error, msg, n);
                    }
                    (false, Obj::Unheld)
                } else {
                    (false, st)
                }
            }
            TimedStmt => match c.items(n).into_iter().find(|&x| c.kind(x) != TimingControl) {
                Some(inner) => self.stmt(inner, st),
                None => (false, st),
            },
            ReturnStmt => {
                if st == Obj::Held {
                    let msg = format!(
                        "Early 'return' in '{}' while phase objection is still held. \
                         Bypassing drop_objection() causes simulation to hang indefinitely.",
                        self.sub
                    );
                    self.add(Severity::Error, msg, n);
                }
                (true, st)
            }
            IfStmt => {
                let Some((_, then, els)) = if_parts(c, n) else { return (false, st) };
                let (tr, to) = self.stmt(then, st);
                let (fr, fo) = match els {
                    Some(e) => self.stmt(e, st),
                    None => (false, st),
                };
                if tr && fr {
                    return (true, to);
                }
                if tr {
                    return (false, fo);
                }
                if fr {
                    return (false, to);
                }
                if to != fo {
                    let msg = format!(
                        "Asymmetric objection state across conditional branch in '{}': 'if' branch leaves objection as {}, while 'else' branch leaves objection as {}. \
                         Unbalanced branch objections lead to unpredictable simulation termination or runtime fatals.",
                        self.sub,
                        obj_str(to),
                        obj_str(fo)
                    );
                    self.add(Severity::Error, msg, n);
                    return (false, Obj::Unknown);
                }
                (false, to)
            }
            CaseStmt if crate::g_uvm::case_keyword(c, n).is_some() => {
                let items = case_items(c, n);
                let mut outs = Vec::new();
                let mut all_return = true;
                let mut has_default = false;
                // slang keeps the default apart and checks it after the items.
                let (defaults, normal): (Vec<_>, Vec<_>) = items.into_iter().partition(|x| x.1);
                for (i, _) in normal.into_iter().chain(defaults) {
                    let (r, o) = self.stmt(i, st);
                    if !r {
                        all_return = false;
                    }
                    outs.push(o);
                    has_default |= false;
                }
                if !c.toks(n).any(|t| c.text_of(t) == "default") {
                    all_return = false;
                    outs.push(st);
                }
                let _ = has_default;
                if all_return {
                    return (true, outs.first().copied().unwrap_or(st));
                }
                if outs.iter().skip(1).any(|&o| o != outs[0]) {
                    let msg = format!(
                        "Asymmetric objection state in case statement inside '{}'. Different case choices result in divergent objection states.",
                        self.sub
                    );
                    self.add(Severity::Error, msg, n);
                    return (false, Obj::Unknown);
                }
                (false, outs.first().copied().unwrap_or(st))
            }
            _ => (false, st),
        }
    }

    fn list(&mut self, items: &[NodeId], st: Obj) -> (bool, Obj) {
        let mut cur = st;
        for &i in items {
            let (r, next) = self.stmt(i, cur);
            if r {
                return (true, next);
            }
            cur = next;
        }
        (false, cur)
    }
}

fn objection_control_flow(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for cls in ix.model_classes() {
        if !is_component(d, cls.id) && !is_sequence(d, cls.id) {
            continue;
        }
        for at in ix.own_routines(cls.id) {
            let c = ix.cst(at);
            let sub = routine_last_name(c, at.node);
            let mut flow = Flow { c, sub: sub.clone(), out: Vec::new() };
            let (ret, st) = flow.list(&body_items(c, at.node), Obj::Unheld);
            for (sev, msg, n) in std::mem::take(&mut flow.out) {
                let Some(t) = c.first_tok(n) else { continue };
                s.add("UVM-OBJ-003", "ObjectionControlFlowMismatch", sev, msg, c.loc(t));
            }
            if !ret && st == Obj::Held {
                let Some(loc) = name_node(c, at.node).and_then(|x| c.tree.token_range(x).last().copied()).map(|t| c.loc(t)) else {
                    continue;
                };
                s.add(
                    "UVM-OBJ-003",
                    "ObjectionControlFlowMismatch",
                    Severity::Error,
                    format!(
                        "Subroutine '{sub}' in class '{}' reaches routine exit while a phase objection remains held. \
                         Every execution path must call 'phase.drop_objection()' before completion to prevent simulation hangs.",
                        cls.name
                    ),
                    loc,
                );
            }
        }
    }
}

fn is_join_none(c: &Cst, n: NodeId) -> bool {
    c.kind(n) == ParBlock && c.tok(n, "join_none").is_some()
}

fn orphan_objection(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for cls in ix.model_classes() {
        if !is_sequence(d, cls.id) {
            continue;
        }
        for at in ix.own_routines(cls.id) {
            let c = ix.cst(at);
            for fork in subtree(c, at.node).into_iter().filter(|&n| is_join_none(c, n)) {
                for thread in block_stmts(c, fork) {
                    let (mut raise, mut drop) = (None, false);
                    for k in calls_in(c, thread) {
                        match call_name(c, k) {
                            "raise_objection" => raise = c.first_tok(k),
                            "drop_objection" => drop = true,
                            _ => {}
                        }
                    }
                    let Some(t) = raise.filter(|_| !drop) else { continue };
                    s.add(
                        "UVM-OBJ-004",
                        "OrphanSubsequenceObjectionHang",
                        Severity::Error,
                        format!(
                            "Asynchronous background process spawned via fork..join_none in sequence '{}' raises an objection without a local drop_objection() call. \
                             When the parent sequence finishes, this orphan objection will remain active, causing a phase timeout simulation hang.",
                            cls.name
                        ),
                        c.loc(t),
                    );
                }
            }
        }
    }
}

fn objection_without_description(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for (ti, c) in csts.iter().enumerate() {
        for n in calls_in(c, c.tree.root) {
            let name = call_name(c, n);
            if name != "raise_objection" && name != "drop_objection" {
                continue;
            }
            if user_code_skip(&ix, c, n) {
                continue;
            }
            let Some(recv) = receiver(c, n) else { continue };
            let cx = ix.ctx(At { tree: ti, node: n });
            let Some(k) = ix.expr_class(c, &cx, recv, 0) else { continue };
            let kc = d.class(k);
            let base = uvm_base_name(d, k);
            if !(kc.name == "uvm_phase" || kc.name == "uvm_objection" || base == "uvm_phase" || base == "uvm_objection") {
                continue;
            }
            // The second argument is the description; an empty literal counts as none.
            let list = arg_list(c, n);
            let ps = list.map(|l| params(c, l)).unwrap_or_default();
            let named = ps.iter().any(|p| p.is_some_and(|a| ordered_expr(c, a).is_none()) || p.is_none());
            let has = if named {
                true
            } else if ps.len() < 2 {
                false
            } else {
                match ps[1].and_then(|a| ordered_expr(c, a)) {
                    Some(e) => string_value(c, e).map_or(true, |v| !v.is_empty()),
                    None => true,
                }
            };
            if has {
                continue;
            }
            let Some(t) = c.first_tok(n) else { continue };
            s.add(
                "UVM-OBJ-005",
                "ObjectionWithoutDescription",
                Severity::Info,
                format!(
                    "{name}() has no description. +UVM_OBJECTION_TRACE then shows only the \
                     component, not what it is waiting for. Pass one, e.g. \
                     phase.{name}(this, \"main traffic sequence\")."
                ),
                c.loc(t),
            );
        }
    }
}

/// `swe::UserCodeVisitor` skips the UVM library and the test stub.
fn user_code_skip(ix: &Ix, c: &Cst, n: NodeId) -> bool {
    let mut cur = Some(n);
    while let Some(p) = cur {
        match c.kind(p) {
            ClassDecl if decl_name(c, p).starts_with("uvm_") => return true,
            PackageDecl if decl_name(c, p) == "uvm_pkg" => return true,
            _ => {}
        }
        cur = c.parent(p);
    }
    let Some(t) = c.first_tok(n) else { return false };
    let l = c.loc(t);
    let f = ix.d.files().get(l.expansion.map_or(l.file, |e| e.0) as usize).map(String::as_str).unwrap_or("");
    f.contains("uvm_mock.svh") || f.contains("uvm_pkg.sv")
}

// ---- observers, scoreboards and TLM ----

fn blocking_in_observer(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for cls in user_classes(d) {
        let mut writes: Vec<String> = Vec::new();
        if class_role(d, cls.id) == Role::Subscriber {
            writes.push("write".into());
        }
        for p in &cls.properties {
            let Some(t) = p.type_class else { continue };
            let n = &d.class(t).name;
            if let Some(sfx) = n.strip_prefix("uvm_analysis_imp") {
                writes.push(format!("write{sfx}"));
            }
        }
        if writes.is_empty() {
            continue;
        }
        for at in ix.all_routines(cls.id) {
            let c = ix.cst(at);
            let name = routine_last_name(c, at.node);
            if !writes.contains(&name) {
                continue;
            }
            let wh = format!("'{name}' of class '{}'", cls.name);
            if c.kind(at.node) == TaskDecl {
                let Some(loc) = name_node(c, at.node).and_then(|x| c.tree.token_range(x).last().copied()).map(|t| c.loc(t)) else {
                    continue;
                };
                s.add(
                    "UVM-OBS-002",
                    "BlockingInObserver",
                    Severity::Error,
                    format!(
                        "Analysis method {wh} is declared as a task. Analysis ports call \
                         write() as a function in zero time; make it a void function and move \
                         time-consuming work to run_phase() behind a uvm_tlm_analysis_fifo."
                    ),
                    loc,
                );
                continue;
            }
            let found = body_items(c, at.node).into_iter().find_map(|i| first_join_none(c, i));
            let Some(fork) = found else { continue };
            let Some(t) = c.first_tok(fork) else { continue };
            s.add(
                "UVM-OBS-002",
                "BlockingInObserver",
                Severity::Warning,
                format!(
                    "Analysis method {wh} forks a thread for every transaction \
                     (fork ... join_none). Threads pile up under load and transactions lose \
                     their order. Push the transaction into a uvm_tlm_analysis_fifo and \
                     process it from one run_phase() thread."
                ),
                c.loc(t),
            );
        }
    }
}

fn first_join_none(c: &Cst, n: NodeId) -> Option<NodeId> {
    subtree(c, n).into_iter().find(|&x| is_join_none(c, x))
}

fn is_scoreboard(d: &Design, cls: &Class) -> bool {
    let l = cls.name.to_ascii_lowercase();
    l.contains("scoreboard") || l.contains("scb") || derived(d, cls.id, "uvm_scoreboard")
}

fn scoreboard_silent_drop(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for cls in ix.model_classes() {
        if !is_scoreboard(d, cls) {
            continue;
        }
        for at in ix.own_routines(cls.id) {
            let c = ix.cst(at);
            let method = routine_last_name(c, at.node);
            if !method.starts_with("write") {
                continue;
            }
            for n in subtree(c, at.node).into_iter().filter(|&n| c.kind(n) == IfStmt) {
                let Some((cond, then, _)) = if_parts(c, n) else { continue };
                let cs = c.text(cond);
                if !(cs.contains(".size()") || cs.contains(".num()") || cs.contains(".empty()")) {
                    continue;
                }
                let body = c.text(then);
                if !body.contains("return") {
                    continue;
                }
                let reported = ["uvm_error", "uvm_fatal", "assert", "error", "fatal", "drop_count"].iter().any(|k| body.contains(k));
                if reported {
                    continue;
                }
                let Some(t) = c.first_tok(n) else { continue };
                s.add(
                    "UVM-SCB-001",
                    "ScoreboardSilentDrop",
                    Severity::Error,
                    format!(
                        "Analysis export method '{method}' in scoreboard class '{}' silently returns when expected queue is empty ('{cs}') without logging an error. If the DUT generates spurious, \
                         misrouted, or unpredicted packets, this silent drop masks the hardware bug. \
                         Report an error with '`uvm_error(\"SPURIOUS_PKT\", ...)' before returning.",
                        cls.name
                    ),
                    c.loc(t),
                );
            }
        }
    }
}

/// The outermost unpacked dimension of a type string (`int$[$]`).
fn outer_dim(ty: &str) -> Option<&str> {
    let at = ty.find("$[")?;
    let rest = &ty[at + 2..];
    let mut depth = 0;
    for (i, ch) in rest.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' if depth == 0 => return Some(&rest[..i]),
            ']' => depth -= 1,
            _ => {}
        }
    }
    None
}

fn is_queue_or_assoc(ty: &str) -> bool {
    let Some(dim) = outer_dim(ty) else { return false };
    if dim == "$" || dim.starts_with("$:") {
        return true;
    }
    if dim.is_empty() {
        return false;
    }
    let fixed = dim.split(':').all(|p| {
        let p = p.trim();
        !p.is_empty() && p.trim_start_matches('-').chars().all(|c| c.is_ascii_digit())
    });
    !fixed
}

fn scoreboard_leftover_queue(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for cls in ix.model_classes() {
        if !is_scoreboard(d, cls) {
            continue;
        }
        let props: Vec<_> = cls
            .properties
            .iter()
            .filter(|p| is_queue_or_assoc(&p.ty))
            .filter(|p| {
                let l = p.name.to_ascii_lowercase();
                l.contains("exp") || l.contains("pred") || l.contains("queue") || l.contains("_q")
            })
            .collect();
        if props.is_empty() {
            continue;
        }
        let mut check = String::new();
        let mut report = String::new();
        for at in ix.own_routines(cls.id) {
            let c = ix.cst(at);
            match routine_last_name(c, at.node).as_str() {
                "check_phase" => check = routine_text(c, at.node),
                "report_phase" => report = routine_text(c, at.node),
                _ => {}
            }
        }
        let raw = format!("{check}\n{report}");
        let mut clean = String::new();
        for line in raw.split('\n') {
            let line = line.find("//").map_or(line, |i| &line[..i]);
            clean.push_str(line);
            clean.push('\n');
        }
        // std::getline does not yield an empty last line.
        if raw.ends_with('\n') {
            clean.pop();
        }
        for p in props {
            let n = &p.name;
            let checked = [".size", ".num", ".empty", " == 0"].iter().any(|sfx| clean.contains(&format!("{n}{sfx}")));
            if checked {
                continue;
            }
            s.add(
                "UVM-SCB-002",
                "ScoreboardLeftoverQueue",
                Severity::Error,
                format!(
                    "Scoreboard class '{}' declares expected transaction container '{n}', but neither 'check_phase' nor 'report_phase' verifies that the container is empty \
                     at the end of simulation. If the DUT drops packets, simulation will complete with 0 errors. \
                     Add 'if ({n}.size() != 0) `uvm_error(\"DROPPED_PKT\", ...)' in check_phase().",
                    cls.name
                ),
                p.loc,
            );
        }
    }
}

/// `swe::normalizedText`: tokens with any trivia between them as one space.
fn normalized(c: &Cst, n: NodeId) -> String {
    let mut out = String::new();
    for t in c.tree.token_range(n) {
        let tok = &c.tree.tokens[t as usize];
        if tok.text.is_empty() {
            continue;
        }
        let has_trivia = if tok.from_macro { tok.space_before } else { !c.tree.trivia(t).is_empty() };
        if !out.is_empty() && has_trivia {
            out.push(' ');
        }
        out.push_str(&tok.text);
    }
    out
}

fn tlm_topology(d: &Design, csts: &[Cst], s: &mut Sink) {
    struct Port<'a> {
        class: &'a str,
        name: &'a str,
        pull: bool,
        loc: Loc,
    }
    let ix = Ix::new(d, csts);
    let mut ports = Vec::new();
    for cls in ix.model_classes() {
        if !is_component(d, cls.id) {
            continue;
        }
        for p in &cls.properties {
            let pull = p.ty.contains("seq_item_pull_port");
            let analysis = p.ty.contains("analysis_port");
            if pull || analysis {
                ports.push(Port { class: &cls.name, name: &p.name, pull, loc: p.loc });
            }
        }
    }
    if ports.is_empty() {
        return;
    }
    let mut conns: Vec<(String, String)> = Vec::new();
    for c in csts {
        for n in c.nodes(CallExpr) {
            if callee(c, n) != "connect" || in_library_class(c, n) {
                continue;
            }
            let text = normalized(c, n);
            let (mut src, mut dst) = (text.clone(), String::new());
            if let Some(at) = text.rfind(".connect") {
                src = text[..at].to_string();
                if let (Some(open), Some(close)) = (text[at..].find('(').map(|i| i + at), text.rfind(')')) {
                    if close > open {
                        dst = text[open + 1..close].to_string();
                    }
                }
            }
            conns.push((src.trim_matches(' ').to_string(), dst.trim_matches(' ').to_string()));
        }
    }
    for p in ports {
        if conns.iter().any(|(a, b)| a.contains(p.name) || b.contains(p.name)) {
            continue;
        }
        let kind = if p.pull { "Driver pull-port" } else { "Analysis port" };
        s.add(
            "UVM-TLM-004",
            "TlmTopologyChecker",
            Severity::Warning,
            format!(
                "{kind} '{}' in component '{}' is declared but never connected to any export or sequencer in connect_phase across the environment. \
                 Unconnected ports will cause null-pointer dereferences or dropped verification transactions.",
                p.name, p.class
            ),
            p.loc,
        );
    }
}

/// Inside a class of the UVM library (or the test stub): `isLibrarySymbol`
/// on the innermost class.
fn in_library_class(c: &Cst, n: NodeId) -> bool {
    let mut cur = Some(n);
    let mut in_class = false;
    while let Some(p) = cur {
        match c.kind(p) {
            ClassDecl => {
                in_class = true;
                if decl_name(c, p).starts_with("uvm_") {
                    return true;
                }
            }
            PackageDecl if decl_name(c, p) == "uvm_pkg" => return in_class,
            _ => {}
        }
        cur = c.parent(p);
    }
    false
}

/// Assignment operators (slang's AssignmentExpression), with `<=` only as
/// a statement.
fn assign_parts(c: &Cst, n: NodeId) -> Option<(NodeId, NodeId)> {
    if c.kind(n) != BinaryExpr {
        return None;
    }
    let op = c.toks(n).next()?;
    let o = c.text_of(op);
    let ok = matches!(o, "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=" | "<<<=" | ">>>=")
        || (o == "<=" && c.parent(n).is_some_and(|p| c.kind(p) == ExprStmt));
    if !ok {
        return None;
    }
    let mut it = c.tree.children(n);
    Some((it.next()?, it.next()?))
}

fn analysis_mutation(_d: &Design, csts: &[Cst], s: &mut Sink) {
    for c in csts {
        for f in c.nodes(FunctionDecl) {
            if routine_last_name(c, f) != "write" {
                continue;
            }
            let Some(pl) = c.tree.child(f, PortList) else { continue };
            let Some(first) = c
                .tree
                .children(pl)
                .filter(|&p| c.kind(p) == Port)
                .find_map(|p| c.tree.child(p, Declarator).and_then(|d| name_node(c, d)))
            else {
                continue;
            };
            let param = c.tree.bare_text(first);
            let prefix = format!("{param}.");
            let spaced = format!(" {param}.");
            for n in subtree(c, f) {
                let Some((lhs, _)) = assign_parts(c, n) else { continue };
                let text = c.text(lhs);
                if !(text.starts_with(&prefix) || text.contains(&spaced)) {
                    continue;
                }
                let Some(t) = c.first_tok(lhs) else { continue };
                s.add(
                    "UVM-TLM-005",
                    "AnalysisTransactionMutation",
                    Severity::Warning,
                    format!(
                        "Direct in-place modification of analysis transaction '{param}' detected in write() method ('{text}'). \
                         Clone or copy the transaction (e.g. $cast(local_tr, {param}.clone())) before modifying its contents to prevent race conditions across broadcast subscribers."
                    ),
                    c.loc(t),
                );
            }
        }
    }
}

fn fifo_drain_starvation(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    let mut fifos = Vec::new();
    for cls in ix.model_classes() {
        if !is_component(d, cls.id) {
            continue;
        }
        for p in &cls.properties {
            if p.ty.contains("tlm_analysis_fifo") || p.ty.contains("tlm_fifo") {
                fifos.push((cls.name.as_str(), p.name.as_str(), p.loc));
            }
        }
    }
    if fifos.is_empty() {
        return;
    }
    let mut connected = HashSet::new();
    let mut drained = HashSet::new();
    for cls in ix.model_classes() {
        for at in ix.own_routines(cls.id) {
            let c = ix.cst(at);
            let is_connect = routine_last_name(c, at.node) == "connect_phase";
            let is_task = c.kind(at.node) == TaskDecl;
            for n in calls_in(c, at.node) {
                let name = call_name(c, n);
                if is_connect && name == "connect" {
                    let text = c.text(n);
                    for f in &fifos {
                        if text.contains(f.1) {
                            connected.insert(f.1);
                        }
                    }
                }
                if is_task && matches!(name, "get" | "try_get" | "peek" | "try_peek") {
                    let text = c.text(n);
                    for f in &fifos {
                        if text.contains(f.1) {
                            drained.insert(f.1);
                        }
                    }
                }
            }
        }
    }
    for (cls, name, loc) in fifos {
        if !connected.contains(name) || drained.contains(name) {
            continue;
        }
        s.add(
            "UVM-TLM-006",
            "AnalysisFifoDrainStarvation",
            Severity::Warning,
            format!(
                "Analysis FIFO '{name}' in component '{cls}' is connected to receive transactions in connect_phase, but is never drained (.get() / .try_get()) \
                 in any task phase across the verification environment. \
                 Unconsumed transactions will queue indefinitely, causing unbounded memory bloat."
            ),
            loc,
        );
    }
}

fn build_phase_cross_access(_d: &Design, csts: &[Cst], s: &mut Sink) {
    for c in csts {
        for f in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)) {
            if routine_last_name(c, f) != "build_phase" {
                continue;
            }
            for n in subtree(c, f) {
                if c.kind(n) != MemberExpr || !member_access_has_syntax(c, n) {
                    continue;
                }
                let text = c.text(n);
                if !(text.contains("m_parent.") || text.contains("parent.")) {
                    continue;
                }
                let Some(first) = text.find('.') else { continue };
                if !text[first + 1..].contains('.') {
                    continue;
                }
                let Some(t) = c.first_tok(n) else { continue };
                s.add(
                    "UVM-PHS-010",
                    "BuildPhaseCrossComponentAccess",
                    Severity::Warning,
                    format!(
                        "Cross-component access '{text}' detected during build_phase. \
                         UVM does not specify the execution order of build_phase across sibling components. \
                         Move cross-component references to connect_phase or pass configurations using uvm_config_db."
                    ),
                    c.loc(t),
                );
            }
        }
    }
}

/// A member access slang keeps syntax for: the outermost part of a dotted
/// name, or `f().x` (not a method being called).
fn member_access_has_syntax(c: &Cst, n: NodeId) -> bool {
    let parent = c.parent(n);
    let called = parent.is_some_and(|p| c.kind(p) == CallExpr && c.tree.children(p).next() == Some(n));
    if called {
        return false;
    }
    if !is_name_chain(c, n) {
        return true;
    }
    !parent.is_some_and(|p| matches!(c.kind(p), MemberExpr | SelectExpr) && c.tree.children(p).next() == Some(n))
}

fn ral_mirror_tampering(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for (ti, c) in csts.iter().enumerate() {
        for n in c.nodes(BinaryExpr) {
            let Some((lhs, _)) = assign_parts(c, n) else { continue };
            if c.kind(lhs) != MemberExpr {
                continue;
            }
            let member = c.last_text(lhs);
            if !matches!(member, "m_value" | "m_mirrored" | "m_desired") {
                continue;
            }
            let Some(value) = c.tree.children(lhs).next() else { continue };
            let cx = ix.ctx(At { tree: ti, node: n });
            let Some(k) = ix.expr_class(c, &cx, value, 0) else { continue };
            let tn = &d.class(k).name;
            let ral = tn.contains("reg")
                || tn.contains("field")
                || derived(d, k, "uvm_reg")
                || derived(d, k, "uvm_reg_field")
                || derived(d, k, "uvm_reg_block");
            if !ral {
                continue;
            }
            let reg = if is_name_chain(c, value) { "reg".to_string() } else { c.text(value) };
            let Some(t) = c.first_tok(lhs) else { continue };
            s.add(
                "UVM-RAL-001",
                "DirectRalMirrorTampering",
                Severity::Error,
                format!(
                    "Direct assignment to internal RAL property '{member}' on register/field handle '{reg}'. \
                     Bypassing the standard UVM RAL API avoids access policy enforcement (RO, WO, W1C), \
                     prediction callbacks, coverage sampling, and mirror consistency checks. \
                     Use 'predict()' to update the mirrored model state, or execute bus read/write operations."
                ),
                c.loc(t),
            );
        }
    }
}

/// An integer literal's value, when it has no x or z bits.
fn int_value(text: &str) -> Option<u128> {
    let t: String = text.chars().filter(|c| *c != '_' && !c.is_whitespace()).collect();
    if let Some(q) = t.find('\'') {
        let rest = &t[q + 1..];
        let rest = rest.trim_start_matches(['s', 'S']);
        let (radix, digits) = match rest.chars().next()? {
            'd' | 'D' => (10, &rest[1..]),
            'h' | 'H' => (16, &rest[1..]),
            'b' | 'B' => (2, &rest[1..]),
            'o' | 'O' => (8, &rest[1..]),
            _ => return None,
        };
        return u128::from_str_radix(digits, radix).ok();
    }
    t.parse().ok()
}

fn low_verbosity_debug(_d: &Design, csts: &[Cst], s: &mut Sink) {
    for c in csts {
        for n in calls_in(c, c.tree.root) {
            let name = call_name(c, n);
            if !(name.contains("uvm_report_info") || name.contains("uvm_info")) {
                continue;
            }
            let a = args(c, n);
            if a.len() < 3 {
                continue;
            }
            let (Some(id), Some(verb)) = (a[0], a[2]) else { continue };
            let idv = string_value(c, id).unwrap_or_default();
            let msg = a[1].and_then(|m| string_value(c, m)).unwrap_or_default();
            let (ui, um) = (idv.to_ascii_uppercase(), msg.to_ascii_uppercase());
            let debug = ui.contains("DEBUG") || ui.contains("TRACE") || ui.contains("DUMP") || um.contains("DEBUG") || um.contains("TRACE");
            if !debug {
                continue;
            }
            let v = unparen(c, verb);
            let mut verb_name = c.text(verb);
            let low = match c.kind(v) {
                NameExpr | ScopedExpr => {
                    let sym = c.last_text(v).to_string();
                    let l = sym == "UVM_LOW" || sym == "UVM_NONE";
                    if l {
                        verb_name = sym;
                    }
                    l
                }
                Literal => int_value(&c.trimmed(v)).is_some_and(|x| x <= 100),
                _ => verb_name.contains("UVM_LOW") || verb_name.contains("UVM_NONE"),
            };
            if !low {
                continue;
            }
            let Some(t) = c.first_tok(n) else { continue };
            s.add(
                "UVM-REP-005",
                "LowVerbosityDebugReport",
                Severity::Warning,
                format!(
                    "UVM report with debug/trace content (ID: '{idv}') is configured with verbosity '{verb_name}'. In UVM logging best practices \
                     (IEEE 1800.2 § 6.3), debug and trace messages must be configured with \
                     UVM_HIGH or UVM_DEBUG to prevent flooding simulation logs during regression runs."
                ),
                c.loc(t),
            );
        }
    }
}

const DISPLAYS: [&str; 12] = [
    "$display", "$displayh", "$displayb", "$displayo", "$write", "$writeh", "$writeb", "$writeo", "$strobe", "$strobeh",
    "$strobeb", "$strobeo",
];

fn display_in_uvm(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for cls in ix.model_classes() {
        let id = cls.id;
        let uvm = derived(d, id, "uvm_report_object")
            || is_component(d, id)
            || is_sequence(d, id)
            || derived(d, id, "uvm_object");
        if !uvm {
            continue;
        }
        for at in ix.own_routines(cls.id) {
            let c = ix.cst(at);
            for n in calls_in(c, at.node) {
                let name = call_name(c, n);
                if !DISPLAYS.contains(&name) {
                    continue;
                }
                let Some(t) = c.first_tok(n) else { continue };
                s.add(
                    "UVM-LOG-002",
                    "ProhibitedDisplayInUvm",
                    Severity::Warning,
                    format!(
                        "Direct system task '{name}' called in UVM class '{}'. Direct display tasks bypass UVM report catchers, severity filtering (+uvm_set_verbosity), and regression log routing. Use `uvm_info or `uvm_error instead.",
                        cls.name
                    ),
                    c.loc(t),
                );
            }
        }
    }
}

fn unchecked_p_sequencer_cast(_d: &Design, csts: &[Cst], s: &mut Sink) {
    for c in csts {
        for st in c.nodes(ExprStmt) {
            let Some(mut e) = stmt_expr(c, st) else { continue };
            if c.kind(e) == CastExpr {
                let items = c.items(e);
                if items.len() != 2 || c.tree.bare_text(items[0]) != "void" {
                    continue;
                }
                e = items[1];
            }
            if c.kind(e) != CallExpr || callee(c, e) != "$cast" {
                continue;
            }
            let a = args(c, e);
            if a.len() < 2 {
                continue;
            }
            let (Some(_), Some(src)) = (a[0], a[1]) else { continue };
            let src = c.text(src);
            let call = c.text(e);
            if !(src.contains("m_sequencer") || call.contains("p_sequencer") || call.contains("m_sequencer")) {
                continue;
            }
            let src = if src.is_empty() { "m_sequencer".to_string() } else { src };
            let Some(t) = c.first_tok(st) else { continue };
            s.add(
                "UVM-SEQ-011",
                "UncheckedPSequencerCast",
                Severity::Warning,
                format!(
                    "Dynamic cast '$cast(p_sequencer, {src})' is executed without checking its return status. \
                     In UVM sequence architecture (IEEE 1800.2 § 5.8), if the sequence is started on an \
                     incompatible sequencer or virtual sequencer, an unchecked $cast leaves 'p_sequencer' null, resulting in an immediate \
                     null-pointer simulation crash. Always check the return value (e.g. 'if (!$cast(p_sequencer, m_sequencer)) `uvm_fatal(...)')."
                ),
                c.loc(t),
            );
        }
    }
}

// ---- factory, config_db and RAL ----

fn global_factory_override(d: &Design, s: &mut Sink) {
    for cls in user_classes(d) {
        let role = class_role(d, cls.id);
        if role == Role::Test || role == Role::None {
            continue;
        }
        for m in &cls.methods {
            if m.is_builtin || m.is_pure {
                continue;
            }
            for k in &m.calls {
                let n = k.name.as_str();
                let owner = k.callee_scope.as_str();
                let global = match owner {
                    "uvm_component" => n == "set_type_override" || n == "set_type_override_by_type",
                    "uvm_factory" | "uvm_default_factory" => {
                        n == "set_type_override_by_type" || n == "set_type_override_by_name"
                    }
                    "uvm_component_registry"
                    | "uvm_object_registry"
                    | "uvm_abstract_component_registry"
                    | "uvm_abstract_object_registry" => n == "set_type_override",
                    _ => false,
                };
                if !global || k.is_system {
                    continue;
                }
                s.add(
                    "UVM-FAC-005",
                    "GlobalFactoryOverridePollution",
                    Severity::Warning,
                    format!(
                        "{n}() in {} '{}' installs a global type override, which changes this type for every \
                         test and VIP in the simulation. Leave type overrides to the test, or use \
                         an instance override (set_inst_override_by_type) scoped to this \
                         component's sub-tree.",
                        role_name(role),
                        cls.name
                    ),
                    k.loc,
                );
            }
        }
    }
}

/// `globMatch` of UvmModel: `*` and `?` wildcards.
fn glob_match(p: &[u8], t: &[u8]) -> bool {
    let (mut pi, mut ti) = (0, 0);
    let (mut star_p, mut star_t) = (None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == b'?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == b'*' {
            star_p = Some(pi);
            pi += 1;
            star_t = ti;
        } else if let Some(sp) = star_p {
            pi = sp + 1;
            star_t += 1;
            ti = star_t;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

fn dangling_config_string(d: &Design, csts: &[Cst], s: &mut Sink) {
    let mut paths = Vec::new();
    for cls in d.classes().iter().filter(|c| !c.is_parameterized) {
        if !is_component(d, cls.id) {
            continue;
        }
        for p in &cls.properties {
            let Some(t) = p.type_class else { continue };
            if is_component(d, t) {
                paths.push(p.name.clone());
                paths.push(format!("uvm_test_top.{}", p.name));
                paths.push(format!("uvm_test_top.m_env.{}", p.name));
            }
        }
    }
    if paths.is_empty() {
        return;
    }
    let matches = |pat: &str| {
        pat.is_empty() || pat == "*" || paths.iter().any(|p| glob_match(pat.as_bytes(), p.as_bytes()) || p.contains(pat))
    };
    for c in csts {
        for n in calls_in(c, c.tree.root) {
            if c.kind(n) != CallExpr {
                continue;
            }
            let name = call_name(c, n);
            let text = c.text(n);
            let left = c.tree.children(n).next().map(|l| c.text(l)).unwrap_or_default();
            let set = (name == "set" && left.contains("uvm_config_db"))
                || (text.contains("uvm_config_db") && text.contains("set"));
            if !set {
                continue;
            }
            let a = args(c, n);
            if a.len() < 4 {
                continue;
            }
            let Some(inst) = a[1] else { continue };
            let Some(pat) = string_value(c, inst) else { continue };
            if pat.is_empty() || pat == "*" || matches(&pat) {
                continue;
            }
            let Some(t) = c.first_tok(inst) else { continue };
            s.add(
                "UVM-DAG-001",
                "UvmDanglingConfigString",
                Severity::Error,
                format!(
                    "Dangling UVM config_db path: string pattern '{pat}' matches 0 instantiated components in the static verification hierarchy DAG. \
                     Typographical errors in config_db target strings cause configurations to be silently dropped."
                ),
                c.loc(t),
            );
        }
    }
}

fn direct_mirror_set(_d: &Design, csts: &[Cst], s: &mut Sink) {
    for c in csts {
        for n in c.nodes(CallExpr) {
            if callee(c, n) != "set" {
                continue;
            }
            let Some(recv) = receiver(c, n) else { continue };
            let recv = unparen(c, recv);
            let target = match c.kind(recv) {
                NameExpr => c.tree.bare_text(recv),
                MemberExpr => c.last_text(recv).to_string(),
                // Slang's element select has no syntax: the name stays "reg".
                SelectExpr => "reg".to_string(),
                _ => c.trimmed(recv),
            };
            if !(target.contains("reg") || target.contains("field") || target.contains("model") || target.contains("ral")) {
                continue;
            }
            let Some(t) = c.first_tok(n) else { continue };
            s.add(
                "RAL-ACC-001",
                "DirectMirrorSetWithoutWrite",
                Severity::Warning,
                format!(
                    "Direct mirror update '{target}.set(...)' modifies only the local software mirror table without issuing a bus transaction to the physical DUT. In UVM RAL architecture (IEEE 1800.2 § 18.5), invoke 'write()' or follow with 'update()' to program physical hardware and maintain register state synchronization."
                ),
                c.loc(t),
            );
        }
    }
}

fn reg_block_never_locked(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    let is_block = |id: ClassId| d.class(id).name != "uvm_reg_block" && derived(d, id, "uvm_reg_block");
    let mut blocks = Vec::new();
    let (mut top, mut sub, mut locked) = (HashSet::new(), HashSet::new(), HashSet::new());
    for cls in d.classes() {
        if is_block(cls.id) && !cls.is_parameterized {
            blocks.push(cls.id);
            if let Some(&at) = ix.decl.get(&cls.id) {
                if ix.cst(at).text(at.node).contains("lock_model") {
                    locked.insert(cls.id);
                }
            }
        }
        for p in &cls.properties {
            let Some(t) = p.type_class.filter(|&t| is_block(t)) else { continue };
            if is_block(cls.id) {
                sub.insert(t);
            } else {
                top.insert(t);
            }
        }
    }
    for (ti, c) in csts.iter().enumerate() {
        for n in c.nodes(CallExpr) {
            if callee(c, n) != "lock_model" {
                continue;
            }
            let Some(recv) = receiver(c, n) else { continue };
            let cx = ix.ctx(At { tree: ti, node: n });
            if let Some(k) = ix.expr_class(c, &cx, recv, 0).filter(|&k| is_block(k)) {
                locked.insert(k);
            }
        }
    }
    let related = |a: ClassId, b: ClassId| a == b || derived(d, a, &d.class(b).name) || derived(d, b, &d.class(a).name);
    for b in blocks {
        let t = top.iter().any(|&x| related(b, x));
        let u = sub.contains(&b);
        let l = locked.iter().any(|&x| related(b, x));
        if !t || u || l {
            continue;
        }
        let cls = d.class(b);
        s.add(
            "RAL-BLK-001",
            "RegBlockNeverLocked",
            Severity::Error,
            format!(
                "Register block '{}' is used as a top-level model but \
                 lock_model() is never called on it. Call lock_model() after build() (in the block's own \
                 build, or where the env creates it) so the address maps are finalized.",
                cls.name
            ),
            cls.loc,
        );
    }
}

/// Names a `X::` scope may start with: classes, packages and typedefs.
fn scope_names(csts: &[Cst]) -> HashSet<String> {
    let mut out: HashSet<String> = ["std", "$unit", "local", "super", "this", "$root"].iter().map(|s| s.to_string()).collect();
    for c in csts {
        for k in [PackageDecl, ClassDecl, ForwardTypedef] {
            for n in c.nodes(k) {
                out.insert(decl_name(c, n));
            }
        }
        for n in c.nodes(TypedefDecl) {
            for x in crate::g_uvm::subtree(c, n) {
                if c.kind(x) == Name {
                    out.insert(c.tree.bare_text(x));
                }
            }
        }
    }
    out
}

/// Slang drops a body with an unresolved `X::` from AST visits.
fn bad_body(c: &Cst, n: NodeId, known: &HashSet<String>) -> bool {
    crate::g_uvm::subtree(c, n).into_iter().any(|x| {
        c.kind(x) == ScopedExpr && c.tree.token_range(x).first().is_some_and(|&t| !known.contains(c.text_of(t)))
    })
}

fn ral_map_overlap(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    let known = scope_names(csts);
    for cls in ix.model_classes() {
        if !(cls.name.contains("reg_block") || derived(d, cls.id, "uvm_reg_block")) {
            continue;
        }
        let width = |name: &str, ty: &str| -> u64 {
            if ty.contains("64") || name.contains("64") {
                8
            } else if ty.contains("16") {
                2
            } else if ty.contains('8') {
                1
            } else {
                4
            }
        };
        let sizes: HashMap<&str, u64> = cls.properties.iter().map(|p| (p.name.as_str(), width(&p.name, &p.ty))).collect();
        let mut entries: Vec<(String, u64, u64, Loc)> = Vec::new();
        for at in ix.own_routines(cls.id) {
            let c = ix.cst(at);
            if bad_body(c, at.node, &known) {
                continue;
            }
            for n in calls_in(c, at.node) {
                if call_name(c, n) != "add_reg" {
                    continue;
                }
                let a = args(c, n);
                if a.len() < 2 {
                    continue;
                }
                let (Some(r), Some(o)) = (a[0], a[1]) else { continue };
                let r = unparen(c, r);
                let reg = if c.kind(r) == NameExpr {
                    c.tree.bare_text(r)
                } else {
                    let t = c.text(r);
                    let t = t.rfind('.').map_or(t.as_str(), |i| &t[i + 1..]).to_string();
                    trim_ws(&t).to_string()
                };
                let Some(off) = offset_value(&c.text(o)) else { continue };
                let size = sizes.get(reg.as_str()).copied().unwrap_or_else(|| {
                    if reg.contains("64") {
                        8
                    } else if reg.contains("16") {
                        2
                    } else if reg.contains('8') {
                        1
                    } else {
                        4
                    }
                });
                let Some(t) = c.first_tok(n) else { continue };
                entries.push((reg, off, size, c.loc(t)));
            }
        }
        for i in 0..entries.len() {
            for j in i + 1..entries.len() {
                let (r1, s1, z1, _) = &entries[i];
                let (r2, s2, z2, l2) = &entries[j];
                let (e1, e2) = (s1.wrapping_add(*z1).wrapping_sub(1), s2.wrapping_add(*z2).wrapping_sub(1));
                if s1 <= &e2 && s2 <= &e1 {
                    s.add(
                        "RAL-MAP-002",
                        "RalMapAddressIntervalOverlap",
                        Severity::Error,
                        format!(
                            "Address interval overlap detected in uvm_reg_map: register '{r2}' ['h{s2}..'h{e2}] collides with previously mapped register '{r1}' ['h{s1}..'h{e1}]. Register address aliasing will cause hardware overwrite corruption."
                        ),
                        *l2,
                    );
                }
            }
        }
    }
}

/// The add_reg offset as the C++ reads it: an integer literal, else
/// `std::stoull` on the text (after `'h`, or with base 0).
fn offset_value(text: &str) -> Option<u64> {
    let t = trim_ws(text);
    if let Some(v) = int_value(t) {
        if !t.contains(['x', 'X', 'z', 'Z', '?']) || !t.contains('\'') {
            return u64::try_from(v).ok();
        }
    }
    if let Some(h) = text.find("'h") {
        return stoull(&text[h + 2..], 16);
    }
    stoull(text, 0)
}

/// `std::stoull`: leading whitespace, then the longest valid prefix.
fn stoull(s: &str, base: u32) -> Option<u64> {
    let s = s.trim_start();
    let (radix, digits) = if base == 0 {
        if let Some(r) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            (16, r)
        } else if s.starts_with('0') && s.len() > 1 {
            (8, &s[1..])
        } else {
            (10, s)
        }
    } else {
        let r = if base == 16 { s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s) } else { s };
        (base, r)
    };
    let end = digits.find(|c: char| !c.is_digit(radix)).unwrap_or(digits.len());
    if end == 0 {
        // "0" alone in base 0 is octal zero.
        return (base == 0 && s.starts_with('0')).then_some(0);
    }
    u64::from_str_radix(&digits[..end], radix).ok()
}

// ---- unused sequences and items ----

fn unused(d: &Design, csts: &[Cst], s: &mut Sink, role: Role, id: &'static str, name: &'static str, noun: &str) {
    let ix = Ix::new(d, csts);
    let users = user_classes(d);
    if !users.iter().any(|c| class_role(d, c.id) == Role::Test) {
        return;
    }
    for cls in &users {
        if class_role(d, cls.id) != role || cls.is_virtual {
            continue;
        }
        if users.iter().any(|h| mentions(&ix, cls, h)) {
            continue;
        }
        s.add(
            id,
            name,
            Severity::Warning,
            format!(
                "{noun} '{}' is never named by another class in this run, so nothing creates or starts it. \
                 Create it from the test, or remove it. Experimental: a use in a file that was \
                 not linted is invisible here.",
                cls.name
            ),
            cls.loc,
        );
    }
}

/// `mentionedFrom`: `host` derives from `target` or names it in a property,
/// a signature or a call's result.
fn mentions(ix: &Ix, target: &Class, host: &Class) -> bool {
    let d = ix.d;
    if host.id == target.id {
        return false;
    }
    if d.ancestors(host.id).any(|a| a.id == target.id) {
        return true;
    }
    let is_t = |ty: &str| class_named(d, type_head(ty)).is_some_and(|k| k.id == target.id);
    if host.properties.iter().any(|p| p.type_class == Some(target.id) || (p.type_class.is_none() && is_t(&p.ty))) {
        return true;
    }
    for m in &host.methods {
        if m.is_builtin {
            continue;
        }
        if is_t(&m.return_type) || m.arguments.iter().any(|a| a.type_class == Some(target.id)) {
            return true;
        }
        // Locals never count: the C++ visitor does not reach them.
    }
    // Calls whose result is the target: T::type_id::create(...) and the like.
    for at in ix.all_routines(host.id) {
        let c = ix.cst(at);
        for n in crate::g_uvm::subtree(c, at.node).into_iter().filter(|&x| c.kind(x) == CallExpr) {
            let Some(left) = c.tree.children(n).next() else { continue };
            if c.kind(left) != ScopedExpr || c.last_text(left) != "create" {
                continue;
            }
            let toks = c.tree.token_range(left);
            if toks.first().is_some_and(|&t| c.text_of(t) == target.name) && c.text(left).contains("type_id") {
                return true;
            }
        }
    }
    false
}

fn unused_sequence(d: &Design, csts: &[Cst], s: &mut Sink) {
    unused(d, csts, s, Role::Sequence, "UVM-SEQ-014", "UnusedSequence", "Sequence");
}

fn unused_sequence_item(d: &Design, csts: &[Cst], s: &mut Sink) {
    unused(d, csts, s, Role::Item, "UVM-SEQ-015", "UnusedSequenceItem", "Sequence item");
}
