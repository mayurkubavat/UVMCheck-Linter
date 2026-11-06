//! Group D of the N6 rule port: dead code, design smells, metrics and
//! architecture rules (SV-DED, SV-DSN, SV-DUP, SV-MET, SV-SML, UVM-ARC,
//! UVM-CFG-006, UVM-ROL-007).
//!
//! The C++ rules walk slang's AST. Here they walk the sv-syntax tree, with a
//! statement view ([`stmt`]) that names the slang statement each node stands
//! for, and a class index ([`crate::g_design_class`]) that plays the part of
//! slang's class symbols. Messages are verbatim. Code inside a module that
//! is instantiated `n` times is reported `n` times, as slang visits each
//! instance body ([`Units::times`]).

use std::collections::HashMap;

use sv_syntax::{Child, NodeId, SyntaxKind};

use crate::syntax::Cst;
use crate::Check::Syntax;
use crate::{Rule, Severity, Sink};
use SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    let mut v = vec![
        Rule {
            id: "SV-DED-002",
            name: "UnreachableStatementsPostReturn",
            check: Syntax(unreachable_after_exit),
        },
        Rule {
            id: "SV-DED-003",
            name: "AccidentalEmptyConditional",
            check: Syntax(empty_conditional),
        },
        Rule {
            id: "SV-DED-004",
            name: "ConsecutiveDeadStore",
            check: Syntax(dead_store),
        },
        Rule {
            id: "SV-DED-007",
            name: "UnusedPureRoutineResult",
            check: Syntax(unused_pure_result),
        },
        Rule {
            id: "SV-DED-008",
            name: "UnusedGenerateBlock",
            check: Syntax(false_condition),
        },
        Rule {
            id: "SV-SML-003",
            name: "GenericReportId",
            check: Syntax(generic_report_id),
        },
        Rule {
            id: "SV-DSN-001",
            name: "TypeSwitchByName",
            check: Syntax(type_switch_by_name),
        },
    ];
    v.extend(crate::g_design_metrics::rules());
    v.extend(crate::g_design_class::rules());
    v.extend(crate::g_design_types::rules());
    v
}

// ---- statements as slang sees them ----

/// The slang statement kind a syntax node stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum St {
    /// `begin ... end`
    Seq,
    /// `fork ... join*`; the join keyword.
    Par(Join),
    If,
    Case,
    RandCase,
    For,
    Foreach,
    While,
    Repeat,
    Forever,
    DoWhile,
    Timed,
    Wait,
    Assert,
    Return,
    Break,
    Continue,
    Expr,
    /// A variable declaration among a block's statements.
    Decl,
    Empty,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Join {
    All,
    Any,
    None,
}

pub(crate) fn stmt(c: &Cst, n: NodeId) -> Option<St> {
    let kw = || c.keyword(n).map(|t| c.text_of(t)).unwrap_or("");
    Some(match c.kind(n) {
        SeqBlock => St::Seq,
        ParBlock => St::Par(match c.toks(n).map(|t| c.text_of(t)).find(|t| t.starts_with("join")) {
            Some("join_any") => Join::Any,
            Some("join_none") => Join::None,
            _ => Join::All,
        }),
        IfStmt => St::If,
        CaseStmt => {
            if c.toks(n).any(|t| c.text_of(t) == "randcase") {
                St::RandCase
            } else {
                St::Case
            }
        }
        LoopStmt => match kw() {
            "for" => St::For,
            "foreach" => St::Foreach,
            "while" => St::While,
            "repeat" => St::Repeat,
            "forever" => St::Forever,
            _ => St::Other,
        },
        DoWhileStmt => St::DoWhile,
        TimedStmt => St::Timed,
        WaitStmt => St::Wait,
        AssertStmt => St::Assert,
        ReturnStmt => St::Return,
        JumpStmt => match kw() {
            "break" => St::Break,
            "continue" => St::Continue,
            _ => St::Other,
        },
        ExprStmt => St::Expr,
        DataDecl => St::Decl,
        EmptyStmt => St::Empty,
        DisableStmt | EventTrigger | ProcAssign | ConcurrentAssert | RandSequenceStmt => St::Other,
        _ => return None,
    })
}

pub(crate) fn is_stmt(c: &Cst, n: NodeId) -> bool {
    stmt(c, n).is_some_and(|s| s != St::Decl)
}

/// Child nodes that are statements or declarations: a block's or routine
/// body's list, in order.
pub(crate) fn list(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.items(n).into_iter().filter(|&x| stmt(c, x).is_some()).collect()
}

/// Statement children of a node (bodies of loops, branches).
pub(crate) fn sub_stmts(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.items(n).into_iter().filter(|&x| is_stmt(c, x)).collect()
}

/// Expression children (not statements, labels or declarations).
pub(crate) fn exprs(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.items(n).into_iter().filter(|&x| stmt(c, x).is_none()).collect()
}

/// An if statement: condition, then-branch, else-branch.
pub(crate) fn if_parts(c: &Cst, n: NodeId) -> (Option<NodeId>, Option<NodeId>, Option<NodeId>) {
    let items = c.items(n);
    let cond = items
        .iter()
        .copied()
        .find(|&x| stmt(c, x).is_none() && c.kind(x) != ElseClause);
    let then = items.iter().copied().find(|&x| is_stmt(c, x));
    let els = items
        .iter()
        .copied()
        .find(|&x| c.kind(x) == ElseClause)
        .and_then(|e| sub_stmts(c, e).into_iter().next());
    (cond, then, els)
}

/// A case statement's selector, items (labels and statement) and default.
pub(crate) struct CaseParts {
    pub sel: Option<NodeId>,
    pub items: Vec<(Vec<NodeId>, NodeId)>,
    pub default: Option<NodeId>,
}

pub(crate) fn case_parts(c: &Cst, n: NodeId) -> CaseParts {
    let mut out = CaseParts {
        sel: None,
        items: Vec::new(),
        default: None,
    };
    let randcase = c.toks(n).any(|t| c.text_of(t) == "randcase");
    let mut after_paren = randcase;
    let mut labels = Vec::new();
    let mut default = false;
    for k in c.kids(n) {
        match *k {
            Child::Token(t) => {
                let s = c.text_of(t);
                if s == ")" && !after_paren {
                    after_paren = true;
                } else if s == "default" && after_paren {
                    default = true;
                }
            }
            Child::Node(x) => {
                if matches!(c.kind(x), StmtLabel | Attribute | EndLabel) {
                    continue;
                }
                if !after_paren {
                    out.sel.get_or_insert(x);
                } else if is_stmt(c, x) {
                    if default {
                        out.default = Some(x);
                        default = false;
                    } else {
                        out.items.push((std::mem::take(&mut labels), x));
                    }
                } else {
                    labels.push(x);
                }
            }
        }
    }
    out
}

/// A for loop's initializers, stop expression, steps and body.
pub(crate) struct ForParts {
    pub init: Vec<NodeId>,
    pub stop: Option<NodeId>,
    pub body: Option<NodeId>,
}

pub(crate) fn for_parts(c: &Cst, n: NodeId) -> ForParts {
    let mut out = ForParts {
        init: Vec::new(),
        stop: None,
        body: None,
    };
    let mut semis = 0;
    let mut closed = false;
    for k in c.kids(n) {
        match *k {
            Child::Token(t) => match c.text_of(t) {
                ";" => semis += 1,
                ")" => closed = true,
                _ => {}
            },
            Child::Node(x) => {
                if matches!(c.kind(x), StmtLabel | Attribute) {
                    continue;
                }
                if closed || (semis >= 2 && is_stmt(c, x) && c.kind(x) != DataDecl) {
                    out.body = Some(x);
                } else if semis == 0 {
                    out.init.push(x);
                } else if semis == 1 {
                    out.stop = Some(x);
                }
            }
        }
    }
    out
}

/// The last statement child: a loop's, timed or wait statement's body.
pub(crate) fn body_of(c: &Cst, n: NodeId) -> Option<NodeId> {
    match stmt(c, n) {
        Some(St::For) => for_parts(c, n).body,
        Some(St::DoWhile) => sub_stmts(c, n).into_iter().next(),
        _ => sub_stmts(c, n).into_iter().last(),
    }
}

/// A binary expression's operator text.
pub(crate) fn op<'t>(c: &Cst<'t>, n: NodeId) -> &'t str {
    if c.kind(n) != BinaryExpr && c.kind(n) != PrefixExpr && c.kind(n) != PostfixExpr {
        return "";
    }
    c.toks(n).next().map(|t| c.text_of(t)).unwrap_or("")
}

pub(crate) const ASSIGN_OPS: &[&str] = &[
    "=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=", "<<<=", ">>>=",
];

/// An assignment as an expression statement: `x = e;`, `x += e;`, `x <= e;`.
pub(crate) fn assignment<'t>(c: &Cst<'t>, s: NodeId) -> Option<(NodeId, NodeId, &'t str)> {
    if c.kind(s) != ExprStmt {
        return None;
    }
    let e = exprs(c, s).into_iter().next()?;
    assign_expr(c, e)
}

pub(crate) fn assign_expr<'t>(c: &Cst<'t>, e: NodeId) -> Option<(NodeId, NodeId, &'t str)> {
    if c.kind(e) != BinaryExpr {
        return None;
    }
    let o = op(c, e);
    if !ASSIGN_OPS.contains(&o) && o != "<=" {
        return None;
    }
    let mut it = c.tree.children(e);
    let (l, r) = (it.next()?, it.next()?);
    Some((l, r, o))
}

/// `x` for a plain identifier expression (slang's NamedValue), not `this`,
/// `super`, `null` or a system name.
pub(crate) fn ident<'t>(c: &Cst<'t>, e: NodeId) -> Option<&'t str> {
    if c.kind(e) != NameExpr {
        return None;
    }
    let mut toks = c.toks(e);
    let t = toks.next()?;
    if toks.next().is_some() {
        return None;
    }
    let s = c.text_of(t);
    let first = s.chars().next()?;
    if !(first.is_ascii_alphabetic() || first == '_' || first == '\\') {
        return None;
    }
    if matches!(s, "this" | "super" | "null" | "local") {
        return None;
    }
    Some(s)
}

/// Every node under `n` (itself included), in source order.
pub(crate) fn subtree(c: &Cst, n: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![n];
    while let Some(x) = stack.pop() {
        out.push(x);
        let kids: Vec<NodeId> = c.tree.children(x).collect();
        stack.extend(kids.into_iter().rev());
    }
    out
}

/// The node's first token location, as slang's `sourceRange.start()`.
pub(crate) fn start(c: &Cst, n: NodeId) -> dv_model::Loc {
    c.first_tok(n).map(|t| c.loc(t)).unwrap_or_default()
}

/// A call: its callee name (last identifier) and the callee node.
pub(crate) fn call_name<'a>(c: &'a Cst, call: NodeId) -> Option<&'a str> {
    if c.kind(call) != CallExpr {
        return None;
    }
    let callee = c.tree.children(call).next()?;
    match c.kind(callee) {
        NameExpr | ScopedExpr | MemberExpr => {
            let (_, b) = c.tree.token_bounds(callee)?;
            Some(c.text_of(b))
        }
        // `x.m() with (...)` nests the call.
        CallExpr => call_name(c, callee),
        _ => None,
    }
}

/// A call's arguments, in order (the Arg nodes' expressions).
pub(crate) fn call_args(c: &Cst, call: NodeId) -> Vec<Option<NodeId>> {
    let Some(list) = c.tree.child(call, ArgList) else {
        return Vec::new();
    };
    c.tree
        .children(list)
        .filter(|&a| c.kind(a) == Arg)
        .map(|a| c.tree.children(a).next())
        .collect()
}

/// File name a token was written in.
pub(crate) fn file_of(c: &Cst, tok: u32) -> String {
    let w = c.tree.tokens[tok as usize].written;
    c.tree
        .files
        .get(w.file as usize)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

// ---- design units and instance counts ----

/// How many times slang elaborates each module, interface or program body.
pub(crate) struct Units {
    times: HashMap<u32, usize>,
    /// Counting only instances outside generate blocks, as
    /// `swe::freeRoutines` walks instance bodies.
    direct: HashMap<u32, usize>,
    /// Specializations of each generic class (by declaration node).
    specs: HashMap<u32, usize>,
    /// Generic class declaration nodes by name, for out-of-block bodies.
    generic_names: HashMap<String, u32>,
}

impl Units {
    pub fn new(c: &Cst) -> Units {
        let specs = crate::g_design_spec::spec_counts(c);
        let generic_names = c
            .nodes(ClassDecl)
            .into_iter()
            .filter(|n| specs.contains_key(&n.0))
            .filter_map(|n| c.tree.child(n, Name).map(|x| (c.tree.bare_text(x), n.0)))
            .collect();
        Units {
            times: Self::count(c, false),
            direct: Self::count(c, true),
            specs,
            generic_names,
        }
    }

    fn count(c: &Cst, direct_only: bool) -> HashMap<u32, usize> {
        let defs: Vec<NodeId> = c
            .nodes(ModuleDecl)
            .into_iter()
            .chain(c.nodes(InterfaceDecl))
            .chain(c.nodes(ProgramDecl))
            .collect();
        let name = |n: NodeId| c.tree.child(n, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default();
        let by_name: HashMap<String, NodeId> = defs.iter().map(|&d| (name(d), d)).collect();
        // Instances of each definition inside each definition.
        let mut edges: HashMap<u32, Vec<(NodeId, usize)>> = HashMap::new();
        let mut instantiated: HashMap<u32, bool> = HashMap::new();
        for inst in c.nodes(Instantiation) {
            let Some(ty) = c.tree.child(inst, DataType) else {
                continue;
            };
            let Some(t) = c.first_tok(ty) else { continue };
            let Some(&def) = by_name.get(c.text_of(t)) else {
                continue;
            };
            let Some(parent) = enclosing_unit(c, inst) else {
                continue;
            };
            if direct_only && in_generate(c, inst, parent) {
                continue;
            }
            let count = c.tree.children(inst).filter(|&x| c.kind(x) == Instance).count();
            edges.entry(parent.0).or_default().push((def, count));
            instantiated.insert(def.0, true);
        }
        let mut times: HashMap<u32, usize> = HashMap::new();
        // Tops are elaborated once; walk down, guarding against cycles.
        fn visit(
            d: NodeId,
            k: usize,
            edges: &HashMap<u32, Vec<(NodeId, usize)>>,
            times: &mut HashMap<u32, usize>,
            depth: usize,
        ) {
            if depth > 32 {
                return;
            }
            *times.entry(d.0).or_insert(0) += k;
            if let Some(es) = edges.get(&d.0) {
                for &(child, n) in es {
                    visit(child, k * n, edges, times, depth + 1);
                }
            }
        }
        for &d in &defs {
            if !instantiated.contains_key(&d.0) {
                visit(d, 1, &edges, &mut times, 0);
            }
        }
        times
    }

    /// [`Self::times`] counting only instances outside generate blocks.
    pub fn direct_times(&self, _c: &Cst, unit: NodeId) -> usize {
        self.direct.get(&unit.0).copied().unwrap_or(0)
    }

    /// How many times the code at `n` is elaborated (0 for a module nobody
    /// reaches from a top, 1 outside modules).
    pub fn times(&self, c: &Cst, n: NodeId) -> usize {
        self.unit_times(c, n) * self.spec_times(c, n)
    }

    /// Instances of the enclosing module only (generic classes once, as
    /// `swe::UserCodeVisitor` walks only their default specialization).
    pub fn unit_times(&self, c: &Cst, n: NodeId) -> usize {
        let mut x = Some(n);
        while let Some(y) = x {
            if matches!(c.kind(y), ModuleDecl | InterfaceDecl | ProgramDecl) {
                return self.times.get(&y.0).copied().unwrap_or(0);
            }
            x = c.parent(y);
        }
        1
    }

    /// Specializations of the generic classes around `n`.
    fn spec_times(&self, c: &Cst, n: NodeId) -> usize {
        let mut k = 1;
        let mut x = Some(n);
        while let Some(y) = x {
            if let Some(&m) = self.specs.get(&y.0) {
                k *= m;
            } else if matches!(c.kind(y), FunctionDecl | TaskDecl) {
                if let Some(nm) = c.tree.child(y, Name) {
                    let toks: Vec<u32> = c.toks(nm).collect();
                    if toks.len() >= 3 && c.text_of(toks[1]) == "::" {
                        if let Some(&g) = self.generic_names.get(c.text_of(toks[0])) {
                            k *= self.specs.get(&g).copied().unwrap_or(1);
                        }
                    }
                }
            }
            x = c.parent(y);
        }
        k
    }
}

fn in_generate(c: &Cst, n: NodeId, unit: NodeId) -> bool {
    let mut x = c.parent(n);
    while let Some(y) = x {
        if y == unit {
            return false;
        }
        if matches!(c.kind(y), GenerateBlock | IfGenerate | LoopGenerate | CaseGenerate) {
            return true;
        }
        x = c.parent(y);
    }
    false
}

pub(crate) fn enclosing_unit(c: &Cst, n: NodeId) -> Option<NodeId> {
    let mut x = c.parent(n);
    while let Some(y) = x {
        if matches!(c.kind(y), ModuleDecl | InterfaceDecl | ProgramDecl) {
            return Some(y);
        }
        x = c.parent(y);
    }
    None
}

/// Adds a finding once per elaboration of the code at `at`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn report(
    s: &mut Sink,
    times: usize,
    rule: &'static str,
    name: &'static str,
    sev: Severity,
    msg: String,
    loc: dv_model::Loc,
) {
    for _ in 0..times {
        s.add(rule, name, sev, msg.clone(), loc);
    }
}

/// Statement lists slang builds: block and routine bodies.
fn statement_lists(c: &Cst) -> Vec<NodeId> {
    c.nodes(SeqBlock)
        .into_iter()
        .chain(c.nodes(ParBlock))
        .chain(c.nodes(Body))
        .collect()
}

// ---- SV-DED-002 ----

fn unreachable_after_exit(c: &Cst, s: &mut Sink) {
    let units = Units::new(c);
    for b in statement_lists(c) {
        let mut term: Option<&str> = None;
        for x in list(c, b) {
            let k = stmt(c, x);
            if let Some(kind) = term {
                if k != Some(St::Empty) {
                    report(
                        s,
                        units.times(c, b),
                        "SV-DED-002",
                        "UnreachableStatementsPostReturn",
                        Severity::Warning,
                        format!(
                            "Unreachable code statement following unconditional control-flow termination ('{kind}'). \
                             Subsequent statements in this block can never execute."
                        ),
                        start(c, x),
                    );
                    break;
                }
            }
            match k {
                Some(St::Return) => term = Some("return"),
                Some(St::Break) => term = Some("break"),
                Some(St::Continue) => term = Some("continue"),
                _ => {}
            }
        }
    }
}

// ---- SV-DED-003 ----

fn empty_conditional(c: &Cst, s: &mut Sink) {
    let units = Units::new(c);
    for n in c.nodes(IfStmt) {
        let (_, then, _) = if_parts(c, n);
        if then.is_some_and(|t| c.kind(t) == EmptyStmt) {
            report(
                s,
                units.times(c, n),
                "SV-DED-003",
                "AccidentalEmptyConditional",
                Severity::Error,
                "Conditional statement 'if (...)' has an empty branch body (stray semicolon ';'). \
                 The subsequent block will execute unconditionally regardless of whether the condition evaluates true or false. \
                 Remove the accidental semicolon after the condition."
                    .into(),
                start(c, n),
            );
        }
    }
    for n in c.nodes(LoopStmt) {
        if stmt(c, n) != Some(St::While) {
            continue;
        }
        if body_of(c, n).is_some_and(|t| c.kind(t) == EmptyStmt) {
            report(
                s,
                units.times(c, n),
                "SV-DED-003",
                "AccidentalEmptyConditional",
                Severity::Error,
                "Loop statement 'while (...)' has an empty body (stray semicolon ';'). \
                 This creates either an unintended zero-delay infinite busy-wait loop or causes subsequent blocks to execute unconditionally."
                    .into(),
                start(c, n),
            );
        }
    }
}

// ---- SV-DED-004 ----

fn dead_store(c: &Cst, s: &mut Sink) {
    let units = Units::new(c);
    let lists = statement_lists(c);
    let poisoned = bad_bodies(c, &lists);
    for b in lists {
        if ancestors_or_self(c, b).any(|x| poisoned.contains(&x.0)) {
            continue;
        }
        let items = list(c, b);
        for w in items.windows(2) {
            let Some((l1, _, _)) = assignment(c, w[0]) else {
                continue;
            };
            let Some(n1) = ident(c, l1) else { continue };
            let Some((l2, r2, _)) = assignment(c, w[1]) else {
                continue;
            };
            if ident(c, l2) != Some(n1) {
                continue;
            }
            // slang drops invalid statements: a syntax error, or a compound
            // assignment to an enum variable.
            if has_error(c, w[0]) || has_error(c, w[1]) {
                continue;
            }
            let reads = subtree(c, r2).into_iter().any(|x| reads_name(c, x, n1));
            if !reads {
                report(
                    s,
                    units.times(c, b),
                    "SV-DED-004",
                    "ConsecutiveDeadStore",
                    Severity::Warning,
                    format!(
                        "Dead store to variable '{n1}'. \
                         The assigned value is unconditionally overwritten by the next assignment statement before ever being read."
                    ),
                    start(c, w[0]),
                );
            }
        }
    }
}

fn ancestors_or_self<'a>(c: &'a Cst, n: NodeId) -> impl Iterator<Item = NodeId> + 'a {
    std::iter::successors(Some(n), move |&x| c.parent(x))
}

/// Outermost statements of procedural bodies that slang cannot bind. A bad
/// statement makes its whole statement list bad, up to the body, and the
/// AST walk skips it. Detected: a compound assignment to an enum variable.
fn bad_bodies(c: &Cst, lists: &[NodeId]) -> std::collections::HashSet<u32> {
    let mut out = std::collections::HashSet::new();
    for &b in lists {
        for st in list(c, b) {
            let Some((l, _, op)) = assignment(c, st) else { continue };
            if op == "=" {
                continue;
            }
            let Some(name) = ident(c, l) else { continue };
            if !enum_typed(c, b, name) {
                continue;
            }
            let mut root = st;
            while let Some(p) = c.parent(root) {
                if matches!(
                    c.kind(p),
                    ProceduralBlock | FunctionDecl | TaskDecl | ClassDecl | ModuleDecl
                ) {
                    break;
                }
                root = p;
            }
            out.insert(root.0);
        }
    }
    out
}

/// The node's tokens hold a syntax error.
fn has_error(c: &Cst, n: NodeId) -> bool {
    let Some((a, b)) = c.tree.token_bounds(n) else {
        return false;
    };
    c.tree.errors.iter().any(|e| e.token >= a && e.token <= b)
}

/// The nearest declaration of `name` around `n` has an enum type.
fn enum_typed(c: &Cst, n: NodeId, name: &str) -> bool {
    let is_enum = |ty: NodeId| -> bool {
        let Some(t) = c.first_tok(ty) else { return false };
        if c.text_of(t) == "enum" {
            return true;
        }
        c.nodes(TypedefDecl).into_iter().any(|td| {
            c.tree
                .child(td, Name)
                .is_some_and(|nm| c.tree.bare_text(nm) == c.text_of(t))
                && c.tree
                    .child(td, DataType)
                    .and_then(|d| c.first_tok(d))
                    .is_some_and(|f| c.text_of(f) == "enum")
        })
    };
    let mut x = Some(n);
    while let Some(y) = x {
        for d in c.tree.children(y).filter(|&k| c.kind(k) == DataDecl) {
            let declares = c
                .tree
                .children(d)
                .filter(|&k| c.kind(k) == Declarator)
                .any(|k| c.tree.child(k, Name).is_some_and(|nm| c.tree.bare_text(nm) == name));
            if declares {
                return c.tree.child(d, DataType).is_some_and(is_enum);
            }
        }
        x = c.parent(y);
    }
    false
}

/// A NamedValue reference to `name`: a bare identifier that is not the
/// member name of `a.b` nor a callee.
fn reads_name(c: &Cst, x: NodeId, name: &str) -> bool {
    if ident(c, x) != Some(name) {
        return false;
    }
    // The callee of `name(...)` is a call, not a value.
    !c.parent(x)
        .is_some_and(|p| c.kind(p) == CallExpr && c.tree.children(p).next() == Some(x))
}

// ---- SV-DED-007 ----

fn unused_pure_result(c: &Cst, s: &mut Sink) {
    for st in c.nodes(ExprStmt) {
        let Some(call) = exprs(c, st).into_iter().next() else {
            continue;
        };
        if c.kind(call) != CallExpr {
            continue;
        }
        let Some(callee) = c.tree.children(call).next() else {
            continue;
        };
        let Some((first, last)) = c.tree.token_bounds(callee) else {
            continue;
        };
        let name = c.text_of(last);
        if name == "$sformatf" {
            s.add(
                "SV-DED-007",
                "UnusedPureRoutineResult",
                Severity::Warning,
                "Result of pure system function '$sformatf()' is discarded in a standalone statement. \
                 In SystemVerilog, $sformatf() returns a formatted string without printing or modifying arguments. \
                 Use $display() to output to the console or $sformat(dest, ...) to write to a string variable."
                    .into(),
                c.loc(first),
            );
        } else if matches!(
            name,
            "$realtime"
                | "$time"
                | "$stime"
                | "$clog2"
                | "$bits"
                | "$typename"
                | "$sin"
                | "$cos"
                | "$tan"
                | "$ln"
                | "$log10"
                | "$exp"
                | "$sqrt"
                | "$pow"
        ) {
            s.add(
                "SV-DED-007",
                "UnusedPureRoutineResult",
                Severity::Warning,
                format!("Result of value-returning system function '{name}()' is discarded in a standalone statement."),
                c.loc(first),
            );
        } else if matches!(
            name,
            "len"
                | "substr"
                | "toupper"
                | "tolower"
                | "compare"
                | "icompare"
                | "atoi"
                | "atohex"
                | "atobin"
                | "atooct"
                | "atoreal"
                | "sum"
                | "product"
        ) {
            s.add(
                "SV-DED-007",
                "UnusedPureRoutineResult",
                Severity::Warning,
                format!("Result of value-returning pure routine '.{name}()' is discarded in a standalone statement."),
                c.loc(last),
            );
        }
    }
}

// ---- SV-DED-008 ----

fn false_condition(c: &Cst, s: &mut Sink) {
    for n in c.nodes(IfStmt) {
        let (cond, _, els) = if_parts(c, n);
        if els.is_some() || c.tree.child(n, ElseClause).is_some() {
            continue;
        }
        let Some(cond) = cond else { continue };
        if c.kind(cond) == MatchExpr {
            continue;
        }
        let text = c.trimmed(cond);
        if matches!(text.as_str(), "0" | "1'b0" | "32'd0" | "false") {
            s.add(
                "SV-DED-008",
                "UnusedGenerateBlock",
                Severity::Info,
                format!(
                    "Conditional statement specifies compile-time false condition '{text}' with no 'else' fallback. \
                     The enclosed code can never be reached or executed under any simulation conditions."
                ),
                start(c, cond),
            );
        }
    }
}

// ---- SV-SML-003 ----

fn generic_report_id(c: &Cst, s: &mut Sink) {
    let units = Units::new(c);
    for call in c.nodes(CallExpr) {
        let Some(name) = call_name(c, call) else { continue };
        let report_call = [
            "uvm_report_info",
            "uvm_report_warning",
            "uvm_report_error",
            "uvm_report_fatal",
            "uvm_info",
            "uvm_error",
            "uvm_warning",
            "uvm_fatal",
        ]
        .iter()
        .any(|k| name.contains(k));
        if !report_call {
            continue;
        }
        let Some(Some(id)) = call_args(c, call).into_iter().next() else {
            continue;
        };
        let Some(val) = string_literal(c, id) else { continue };
        let upper = val.to_ascii_uppercase();
        if matches!(
            upper.as_str(),
            "ERR" | "ERROR" | "INFO" | "WARN" | "WARNING" | "DEBUG" | "FATAL" | "TEST" | "TB" | "MSG" | "TEMP" | ""
        ) {
            report(
                s,
                units.times(c, call),
                "SV-SML-003",
                "GenericReportId",
                Severity::Info,
                format!(
                    "Generic or cryptic UVM report ID '{val}'. Using generic single-word report IDs prevents targeted log filtering and severity overrides via '+uvm_set_severity' and 'uvm_report_catcher'. \
                     Use descriptive, feature-scoped identifiers (e.g. 'AXI_SLV_TIMEOUT', 'PCIE_TLP_CRC_ERR')."
                ),
                start(c, id),
            );
        }
    }
}

/// The value of a string literal expression (escapes as written).
pub(crate) fn string_literal(c: &Cst, e: NodeId) -> Option<String> {
    if c.kind(e) != Literal {
        return None;
    }
    let mut toks = c.toks(e);
    let t = c.text_of(toks.next()?);
    if toks.next().is_some() || !t.starts_with('"') || t.len() < 2 {
        return None;
    }
    Some(unescape(&t[1..t.len() - 1]))
}

fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(ch) = it.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    out
}

// ---- SV-DSN-001 ----

fn type_switch_by_name(c: &Cst, s: &mut Sink) {
    let units = Units::new(c);
    let classes = crate::g_design_class::Classes::new(c);
    let is_name_call =
        |e: NodeId| call_name(c, strip_parens(c, e)) == Some("get_type_name") && c.kind(strip_parens(c, e)) == CallExpr;
    let is_str = |e: NodeId| string_literal(c, strip_parens(c, e)).is_some();
    let msg = "Behavior is chosen by comparing get_type_name() with a string. Every new type then needs \
               edits here, and a renamed class silently falls through. Move the behavior into a virtual \
               method of the class hierarchy (open/closed principle).";
    let mut found: Vec<NodeId> = Vec::new();
    for n in c.nodes(BinaryExpr) {
        if !matches!(op(c, n), "==" | "!=" | "===" | "!==") {
            continue;
        }
        let mut it = c.tree.children(n);
        let (Some(l), Some(r)) = (it.next(), it.next()) else {
            continue;
        };
        if (is_name_call(l) && is_str(r)) || (is_name_call(r) && is_str(l)) {
            found.push(n);
        }
    }
    for n in c.nodes(CaseStmt) {
        if stmt(c, n) != Some(St::Case) {
            continue;
        }
        if let Some(sel) = case_parts(c, n).sel {
            if is_name_call(sel) {
                found.push(sel);
            }
        }
    }
    for n in found {
        if classes.in_library(c, n) {
            continue;
        }
        report(
            s,
            units.unit_times(c, n),
            "SV-DSN-001",
            "TypeSwitchByName",
            Severity::Warning,
            msg.into(),
            start(c, n),
        );
    }
}

/// Looks through parentheses, which slang keeps out of the AST.
pub(crate) fn strip_parens(c: &Cst, e: NodeId) -> NodeId {
    let mut e = e;
    while c.kind(e) == ParenExpr {
        match c.tree.children(e).next() {
            Some(x) => e = x,
            None => break,
        }
    }
    e
}
