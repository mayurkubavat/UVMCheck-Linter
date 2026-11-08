//! Group A rules that read method bodies: who creates what, config_db and
//! factory calls, stimulus in drivers (MG-3, MG-4).

use std::collections::{BTreeSet, HashMap};

use dv_model::{Call, Class, Design, Loc, Method};
use sv_lexer::Kind;
use sv_syntax::{Child, NodeId};

use crate::g_methodology::{class_methods, is_component_role, key, role, role_name, user_classes, Ctx, Index, Role};
use crate::syntax::Cst;
use crate::Check::Mixed;
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "UVM-ROL-001", name: "DriverGeneratesStimulus", check: Mixed(driver_generates_stimulus) },
        Rule { id: "UVM-ROL-004", name: "TestBuildsLeafComponents", check: Mixed(test_builds_leaf_components) },
        Rule { id: "UVM-RUS-001", name: "AgentIgnoresIsActive", check: Mixed(agent_ignores_is_active) },
        Rule { id: "UVM-RUS-002", name: "AgentWithoutConfigObject", check: Mixed(agent_without_config_object) },
        Rule { id: "UVM-RUS-003", name: "HardcodedTopology", check: Mixed(hardcoded_topology) },
        Rule { id: "UVM-RUS-004", name: "AbsoluteConfigDbPath", check: Mixed(absolute_config_db_path) },
    ]
}

// ---- statements ----

/// The statements of a block or body (declarations included, as slang's
/// statement lists hold them).
pub(crate) fn stmts(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.items(n)
}

/// `if (cond) then [else other]`: condition, then and else statements.
pub(crate) fn if_parts(c: &Cst, n: NodeId) -> (Option<NodeId>, Option<NodeId>, Option<NodeId>) {
    let kids = c.items(n);
    let cond = kids.first().copied();
    let then = kids.get(1).copied().filter(|&x| c.kind(x) != ElseClause);
    let other = c.tree.child(n, ElseClause).and_then(|e| c.items(e).into_iter().next());
    (cond, then, other)
}

/// A case statement: its expression, and each item's labels and statement
/// (and whether it is the `default` item).
pub(crate) struct CaseParts {
    pub expr: Option<NodeId>,
    pub items: Vec<(Vec<NodeId>, Option<NodeId>, bool)>,
}

pub(crate) fn case_parts(c: &Cst, n: NodeId) -> CaseParts {
    let mut expr = None;
    let mut items = Vec::new();
    let mut labels = Vec::new();
    let mut depth = 0;
    let mut closed = false;
    let mut default = false;
    let mut after_colon = false;
    for k in c.kids(n) {
        match *k {
            Child::Token(t) => match c.text_of(t) {
                "(" if !closed => depth += 1,
                ")" if !closed => {
                    depth -= 1;
                    if depth == 0 {
                        closed = true;
                    }
                }
                ":" if closed => after_colon = true,
                "default" if closed => default = true,
                _ => {}
            },
            Child::Node(x) => {
                if matches!(c.kind(x), StmtLabel | Attribute | EndLabel) {
                    continue;
                }
                if !closed {
                    expr = Some(x);
                } else if after_colon || (default && labels.is_empty() && is_stmt(c, x)) {
                    items.push((std::mem::take(&mut labels), Some(x), default));
                    default = false;
                    after_colon = false;
                } else {
                    labels.push(x);
                }
            }
        }
    }
    CaseParts { expr, items }
}

pub(crate) fn is_stmt(c: &Cst, n: NodeId) -> bool {
    matches!(
        c.kind(n),
        SeqBlock
            | ParBlock
            | IfStmt
            | CaseStmt
            | LoopStmt
            | DoWhileStmt
            | TimedStmt
            | WaitStmt
            | AssertStmt
            | EmptyStmt
            | ExprStmt
            | ReturnStmt
            | JumpStmt
            | DisableStmt
            | EventTrigger
            | ProcAssign
            | RandSequenceStmt
    )
}

/// A loop: its keyword, the slots between its parentheses (split at `;`)
/// and its body.
pub(crate) fn loop_parts<'t>(c: &Cst<'t>, n: NodeId) -> (&'t str, Vec<Option<NodeId>>, Option<NodeId>) {
    let kw = c.keyword(n).map(|t| c.text_of(t)).unwrap_or("");
    let mut slots: Vec<Option<NodeId>> = vec![None];
    let mut depth = 0;
    let mut body = None;
    let mut closed = false;
    for k in c.kids(n) {
        match *k {
            Child::Token(t) => match c.text_of(t) {
                "(" if !closed => depth += 1,
                ")" if !closed => {
                    depth -= 1;
                    if depth == 0 {
                        closed = true;
                    }
                }
                ";" if depth == 1 && !closed => slots.push(None),
                _ => {}
            },
            Child::Node(x) => {
                if matches!(c.kind(x), StmtLabel | Attribute) {
                    continue;
                }
                if closed || (depth == 0 && kw == "forever") {
                    body = Some(x);
                } else if let Some(s @ None) = slots.last_mut() {
                    *s = Some(x);
                }
            }
        }
    }
    (kw, slots, body)
}

/// `swe::alwaysExits`.
pub(crate) fn always_exits(c: &Cst, n: NodeId) -> bool {
    match c.kind(n) {
        ReturnStmt => true,
        JumpStmt => c.keyword(n).is_some_and(|t| matches!(c.text_of(t), "break" | "continue")),
        ExprStmt => c.items(n).first().is_some_and(|&e| is_stop_call(c, e)),
        SeqBlock => stmts(c, n).into_iter().any(|s| always_exits(c, s)),
        ParBlock => {
            if c.toks(n).any(|t| c.text_of(t) == "join_none") {
                return false;
            }
            stmts(c, n).into_iter().any(|s| always_exits(c, s))
        }
        IfStmt => {
            let (_, then, other) = if_parts(c, n);
            let then_exits = then.is_some_and(|t| always_exits(c, t));
            if is_report_guard(c, n) && then_exits {
                return true;
            }
            then_exits && other.is_some_and(|o| always_exits(c, o))
        }
        CaseStmt => {
            let p = case_parts(c, n);
            p.items.iter().any(|i| i.2) && p.items.iter().all(|i| i.1.is_some_and(|s| always_exits(c, s)))
        }
        TimedStmt => c
            .items(n)
            .into_iter()
            .find(|&x| c.kind(x) != TimingControl)
            .is_some_and(|s| always_exits(c, s)),
        _ => false,
    }
}

fn is_stop_call(c: &Cst, e: NodeId) -> bool {
    let e = c.strip(e);
    c.kind(e) == CallExpr && matches!(c.callee_name(e), "$fatal" | "$finish" | "$exit" | "uvm_report_fatal")
}

/// Calls in a subtree, in pre-order (outer before inner).
pub(crate) fn calls_in(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.subtree(n).into_iter().filter(|&x| c.kind(x) == CallExpr && !is_with_wrapper(c, x)).collect()
}

/// `f(...) with ...`: the outer node of a call with a `with` clause.
pub(crate) fn is_with_wrapper(c: &Cst, n: NodeId) -> bool {
    c.tree.children(n).next().is_some_and(|x| c.kind(x) == CallExpr)
}

/// `swe::isReportGuard`: an `if` whose condition only queries reporting.
pub(crate) fn is_report_guard(c: &Cst, n: NodeId) -> bool {
    let (Some(cond), _, _) = if_parts(c, n) else { return false };
    let (mut report, mut other) = (false, false);
    for call in calls_in(c, cond) {
        let name = c.callee_name(call);
        if matches!(name, "uvm_report_enabled" | "get_report_verbosity_level" | "get_report_action" | "uvm_get_report_object") {
            report = true;
        } else {
            other = true;
        }
    }
    report && !other
}

/// `meth::mentionsActive`.
pub(crate) fn mentions_active(c: &Cst, cond: NodeId) -> bool {
    let t = c.expr_text(cond).to_ascii_lowercase();
    t.contains("active") || t.contains("passive")
}

fn is_active_early_exit(c: &Cst, s: NodeId) -> bool {
    if c.kind(s) != IfStmt {
        return false;
    }
    let (cond, then, other) = if_parts(c, s);
    other.is_none() && cond.is_some_and(|x| mentions_active(c, x)) && then.is_some_and(|t| always_exits(c, t))
}

// ---- calls, with what the model knows of them ----

type CallKey<'d> = ((u32, u32, u32, Option<(u32, u32)>), &'d str);

/// The model's calls of a method, by where they start and their name.
pub(crate) struct Calls<'d> {
    by: HashMap<CallKey<'d>, &'d Call>,
}

impl<'d> Calls<'d> {
    pub fn new(m: &'d Method) -> Self {
        let mut by = HashMap::new();
        for k in &m.calls {
            by.entry((key(k.loc), k.name.as_str())).or_insert(k);
        }
        Calls { by }
    }

    pub fn get(&self, c: &Cst, call: NodeId) -> Option<&'d Call> {
        self.by.get(&(key(c.loc_of(call)), c.callee_name(call))).copied()
    }

    /// The class declaring the callee (`uvm_config_db`), or "".
    pub fn owner(&self, c: &Cst, call: NodeId) -> &'d str {
        self.get(c, call).filter(|k| !k.is_system).map(|k| k.callee_scope.as_str()).unwrap_or("")
    }
}

/// `meth::isConfigDbCall`.
pub(crate) fn is_config_db_call(calls: &Calls, c: &Cst, call: NodeId, method: &str) -> bool {
    c.callee_name(call) == method && calls.owner(c, call) == "uvm_config_db"
}

// ---- creations (meth::creationsIn) ----

pub(crate) struct Creation<'d> {
    pub class: &'d Class,
    pub loc: Loc,
    pub guarded: bool,
}

/// Every `T::type_id::create(...)` and `new` under a statement, in source
/// order, with whether an active/passive test guards it.
pub(crate) fn creations<'d>(ctx: &Ctx<'d, '_>, calls: &Calls<'d>, c: &Cst, n: NodeId) -> Vec<Creation<'d>> {
    let mut out = Vec::new();
    walk_creations(ctx, calls, c, n, false, &mut out);
    out
}

fn walk_creations<'d>(ctx: &Ctx<'d, '_>, calls: &Calls<'d>, c: &Cst, n: NodeId, guarded: bool, out: &mut Vec<Creation<'d>>) {
    match c.kind(n) {
        IfStmt => {
            let (cond, then, other) = if_parts(c, n);
            let mut g = guarded;
            if let Some(x) = cond {
                walk_creations(ctx, calls, c, x, g, out);
                if mentions_active(c, x) {
                    g = true;
                }
            }
            for s in [then, other].into_iter().flatten() {
                walk_creations(ctx, calls, c, s, g, out);
            }
            return;
        }
        CaseStmt => {
            let p = case_parts(c, n);
            let mut g = guarded;
            if let Some(x) = p.expr {
                walk_creations(ctx, calls, c, x, g, out);
                if mentions_active(c, x) {
                    g = true;
                }
            }
            for s in p.items.into_iter().filter_map(|i| i.1) {
                walk_creations(ctx, calls, c, s, g, out);
            }
            return;
        }
        SeqBlock | ParBlock | Body => {
            let mut g = guarded;
            for s in stmts(c, n) {
                walk_creations(ctx, calls, c, s, g, out);
                if is_active_early_exit(c, s) {
                    g = true;
                }
            }
            return;
        }
        CallExpr if !is_with_wrapper(c, n) => {
            if let Some(cls) = factory_create_type(ctx, calls, c, n).or_else(|| scoped_new_type(ctx, c, n)) {
                out.push(Creation { class: cls, loc: c.loc_of(n), guarded });
            }
        }
        NewExpr => {
            if let Some(cls) = new_type(ctx, c, n) {
                out.push(Creation { class: cls, loc: c.loc_of(n), guarded });
            }
        }
        _ => {}
    }
    for k in c.node_kids(n) {
        walk_creations(ctx, calls, c, k, guarded, out);
    }
}

/// `meth::isFactoryCreate`, and the class it creates.
pub(crate) fn factory_create_type<'d>(ctx: &Ctx<'d, '_>, calls: &Calls<'d>, c: &Cst, call: NodeId) -> Option<&'d Class> {
    if c.callee_name(call) != "create" {
        return None;
    }
    let owner = calls.owner(c, call);
    if owner != "uvm_component_registry" && owner != "uvm_object_registry" {
        return None;
    }
    let callee = c.tree.children(call).next()?;
    if c.kind(callee) != ScopedExpr {
        return None;
    }
    // T::type_id::create: the registry's T.
    let reg = c.tree.children(callee).next()?;
    match c.kind(reg) {
        ScopedExpr => {
            let t = c.tree.children(reg).next()?;
            ctx.class_of_type(&c.tree.bare_text(t))
        }
        // type_id::create inside the class itself.
        NameExpr if c.tree.bare_text(reg) == "type_id" => Some(ctx.cls),
        _ => None,
    }
}

/// `T::new(...)`.
fn scoped_new_type<'d>(ctx: &Ctx<'d, '_>, c: &Cst, call: NodeId) -> Option<&'d Class> {
    let callee = c.tree.children(call).next()?;
    if c.kind(callee) != ScopedExpr || c.last_text(callee) != "new" {
        return None;
    }
    let t = c.tree.children(callee).next()?;
    if c.tree.bare_text(t) == "super" {
        return None;
    }
    ctx.class_of_type(&c.tree.bare_text(t))
}

/// The class a `new` constructs: the type it is assigned to.
pub(crate) fn new_type<'d>(ctx: &Ctx<'d, '_>, c: &Cst, n: NodeId) -> Option<&'d Class> {
    if c.tok(n, "[").is_some() || c.tree.children(n).any(|x| c.kind(x) != ArgList) {
        return None;
    }
    let p = c.parent(n)?;
    match c.kind(p) {
        BinaryExpr if c.toks(p).next().is_some_and(|t| c.text_of(t) == "=") => {
            let lhs = c.tree.children(p).next()?;
            if lhs == n {
                return None;
            }
            ctx.class_of(c, lhs)
        }
        Declarator => {
            let decl = c.parent(p)?;
            let ty = c.tree.child(decl, DataType)?;
            ctx.class_of_type(&c.tree.bare_text(ty))
        }
        ReturnStmt => ctx.m.and_then(|m| ctx.class_of_type(&m.return_type)),
        _ => None,
    }
}

/// Every method of every user class with a body in the trees.
pub(crate) fn bodies<'a, 'd, 't>(d: &'d Design, ix: &Index<'a, 't>) -> Vec<(&'d Class, &'d Method, &'a Cst<'t>, NodeId)> {
    let mut v = Vec::new();
    for cls in user_classes(d) {
        for m in class_methods(cls) {
            if let Some((c, b)) = ix.body(m) {
                v.push((cls, m, c, b));
            }
        }
    }
    v
}

// ---- rules ----

/// The object a call works on: `obj` of `obj.f()`, the first argument of
/// `std::randomize(obj)`.
fn receiver_of(c: &Cst, call: NodeId) -> Option<NodeId> {
    let callee = c.tree.children(call).next()?;
    match c.kind(callee) {
        MemberExpr => c.tree.children(callee).next(),
        ScopedExpr if c.tree.bare_text(callee) == "std::randomize" => c.arg_expr(c.call_args(call).first().copied().flatten()),
        _ => None,
    }
}

/// `exprText(*receiverOf(call))`: slang's receiver of `obj.f()` has no
/// syntax of its own, so only `std::randomize(obj)` names it.
fn receiver_text(c: &Cst, call: NodeId, r: NodeId) -> String {
    let callee = c.tree.children(call).next();
    if callee.is_some_and(|x| c.kind(x) == MemberExpr) {
        String::new()
    } else {
        c.expr_text(r)
    }
}

fn driver_generates_stimulus(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for (cls, m, c, body) in bodies(d, &ix) {
        if role(d, cls) != Role::Driver {
            continue;
        }
        let ctx = Ctx { d, cls, m: Some(m), td: Some(&ix.typedefs) };
        for call in calls_in(c, body) {
            let n = c.callee_name(call);
            if n != "randomize" && n != "start" {
                continue;
            }
            let Some(r) = receiver_of(c, call) else { continue };
            let rr = ctx.class_of(c, r).map(|k| role(d, k)).unwrap_or(Role::None);
            let what = if n == "randomize" && (rr == Role::Item || rr == Role::Sequence) {
                format!("randomizes {} '{}'", role_name(rr), receiver_text(c, call, r))
            } else if n == "start" && rr == Role::Sequence {
                format!("starts sequence '{}'", receiver_text(c, call, r))
            } else {
                continue;
            };
            out.add(
                "UVM-ROL-001",
                "DriverGeneratesStimulus",
                Severity::Warning,
                format!(
                    "Driver '{}' {} in {}(). Stimulus generated inside the driver cannot be constrained or replaced by tests; move it into a sequence and let the \
driver only drive the items it receives.",
                    cls.name, what, m.name
                ),
                c.loc_of(call),
            );
        }
    }
}

pub(crate) fn class_creations<'d>(d: &'d Design, ix: &Index, cls: &'d Class) -> Vec<Creation<'d>> {
    let mut v = Vec::new();
    for m in class_methods(cls) {
        let Some((c, body)) = ix.body(m) else { continue };
        let ctx = Ctx { d, cls, m: Some(m), td: Some(&ix.typedefs) };
        let calls = Calls::new(m);
        v.extend(creations(&ctx, &calls, c, body));
    }
    v
}

fn test_builds_leaf_components(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for cls in user_classes(d) {
        if role(d, cls) != Role::Test {
            continue;
        }
        for cr in class_creations(d, &ix, cls) {
            let r = role(d, cr.class);
            if !matches!(r, Role::Driver | Role::Monitor | Role::Sequencer | Role::Agent | Role::Scoreboard | Role::Subscriber) {
                continue;
            }
            out.add(
                "UVM-ROL-004",
                "TestBuildsLeafComponents",
                Severity::Warning,
                format!(
                    "Test '{}' creates {} '{}' itself. Each test that builds its own components drifts from the others; create an env that builds them, and \
change it from the test through its config object or factory overrides.",
                    cls.name,
                    role_name(r),
                    cr.class.name
                ),
                cr.loc,
            );
        }
    }
}

fn agent_ignores_is_active(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for cls in user_classes(d) {
        if role(d, cls) != Role::Agent {
            continue;
        }
        for cr in class_creations(d, &ix, cls) {
            let r = role(d, cr.class);
            if cr.guarded || (r != Role::Driver && r != Role::Sequencer) {
                continue;
            }
            out.add(
                "UVM-RUS-001",
                "AgentIgnoresIsActive",
                Severity::Warning,
                format!(
                    "Agent '{}' creates {} '{}' without checking get_is_active(). In a passive agent the driver would still fight the DUT for the bus. Create the \
driver and sequencer only inside 'if (get_is_active() == UVM_ACTIVE)'.",
                    cls.name,
                    role_name(r),
                    cr.class.name
                ),
                cr.loc,
            );
        }
    }
}

/// RUS-002's value types it leaves out: a class handle, a virtual
/// interface or an unpacked array.
fn is_handle_or_array(ctx: &Ctx<'_, '_>, c: &Cst, e: NodeId) -> bool {
    let e = c.strip(e);
    if ctx.class_of(c, e).is_some() {
        return true;
    }
    let name = match c.kind(e) {
        NameExpr if c.tree.children(e).next().is_none() => c.tree.bare_text(e),
        _ => return false,
    };
    let aggregate = |ty: &str| ty.contains('$') || ty.starts_with("virtual");
    if let Some(m) = ctx.m {
        if let Some(l) = m.locals.iter().find(|l| l.name == name) {
            return l.is_virtual_interface || aggregate(&l.ty);
        }
        if let Some(a) = m.arguments.iter().find(|a| a.name == name) {
            return aggregate(&a.ty);
        }
    }
    std::iter::once(ctx.cls)
        .chain(ctx.d.ancestors(ctx.cls.id))
        .find_map(|k| k.properties.iter().find(|p| p.name == name))
        .is_some_and(|p| p.is_virtual_interface || aggregate(&p.ty))
}

fn agent_without_config_object(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    const LIMIT: usize = 2;
    for cls in user_classes(d) {
        let r = role(d, cls);
        if r != Role::Agent && r != Role::Env {
            continue;
        }
        let mut fields = BTreeSet::new();
        let mut first = None;
        for m in class_methods(cls) {
            let Some((c, body)) = ix.body(m) else { continue };
            let ctx = Ctx { d, cls, m: Some(m), td: Some(&ix.typedefs) };
            let calls = Calls::new(m);
            for call in calls_in(c, body) {
                let args = c.call_args(call);
                if !is_config_db_call(&calls, c, call, "get") || args.len() < 4 {
                    continue;
                }
                let (Some(field), Some(value)) = (c.arg_expr(args[2]), c.arg_expr(args[3])) else { continue };
                if is_handle_or_array(&ctx, c, value) {
                    continue;
                }
                let f = c.string_value(field).unwrap_or_else(|| c.expr_text(field));
                if fields.insert(f) && fields.len() == 1 {
                    first = Some(c.loc_of(call));
                }
            }
        }
        if fields.len() <= LIMIT {
            continue;
        }
        let list = fields.iter().cloned().collect::<Vec<_>>().join(", ");
        out.add(
            "UVM-RUS-002",
            "AgentWithoutConfigObject",
            Severity::Warning,
            format!(
                "{} '{}' reads {} separate settings from uvm_config_db ({}). Settings that travel together belong in one config object: it is typed, \
documented in one place, can be randomized and is set with one call.",
                role_name(r),
                cls.name,
                fields.len(),
                list
            ),
            first.unwrap_or(cls.loc),
        );
    }
}

/// An integer literal's text (slang's IntegerLiteral: not a string, real,
/// time or unbased literal).
pub(crate) fn integer_literal(c: &Cst, e: NodeId) -> Option<String> {
    let e = c.strip(e);
    if c.kind(e) != Literal {
        return None;
    }
    let toks = c.tree.token_range(e);
    let ok = toks
        .iter()
        .all(|&t| matches!(c.tree.tokens[t as usize].kind, Kind::IntegerLiteral | Kind::IntegerBase | Kind::VectorDigits));
    (ok && !toks.is_empty()).then(|| c.expr_text(e))
}

fn hardcoded_topology(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for (cls, m, c, body) in bodies(d, &ix) {
        if role(d, cls) != Role::Env {
            continue;
        }
        let ctx = Ctx { d, cls, m: Some(m), td: Some(&ix.typedefs) };
        let calls = Calls::new(m);
        for l in c.subtree(body).into_iter().filter(|&x| c.kind(x) == LoopStmt) {
            let (kw, slots, lbody) = loop_parts(c, l);
            let bound = match kw {
                "repeat" => slots.first().copied().flatten().and_then(|e| integer_literal(c, e)),
                "for" => slots.get(1).copied().flatten().and_then(|stop| {
                    let stop = c.strip(stop);
                    (c.kind(stop) == BinaryExpr)
                        .then(|| c.tree.children(stop).nth(1))
                        .flatten()
                        .and_then(|r| integer_literal(c, r))
                }),
                _ => None,
            };
            let (Some(bound), Some(lbody)) = (bound, lbody) else { continue };
            if let Some(cr) = creations(&ctx, &calls, c, lbody).into_iter().find(|cr| is_component_role(role(d, cr.class))) {
                out.add(
                    "UVM-RUS-003",
                    "HardcodedTopology",
                    Severity::Warning,
                    format!(
                        "Env '{}' creates {} instances of '{}' with a literal loop count. The same env cannot then be reused with a different number of interfaces; take the count \
(and per-instance settings) from the env's config object.",
                        cls.name, bound, cr.class.name
                    ),
                    c.loc_of(l),
                );
            }
        }
    }
}

fn absolute_config_db_path(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for (cls, m, c, body) in bodies(d, &ix) {
        let r = role(d, cls);
        let calls = Calls::new(m);
        for call in calls_in(c, body) {
            let is_set = is_config_db_call(&calls, c, call, "set");
            let args = c.call_args(call);
            if (!is_set && !is_config_db_call(&calls, c, call, "get")) || args.len() < 2 {
                continue;
            }
            let path = c.arg_expr(args[1]).and_then(|a| c.string_value(a)).unwrap_or_default();
            let p = path.trim_start_matches(['*', '.']);
            if !p.starts_with("uvm_test_top") {
                continue;
            }
            let null_context = c.arg_expr(args[0]).is_some_and(|a| c.is_null(a));
            if null_context && r == Role::Test {
                continue;
            }
            let msg = if null_context {
                format!(
                    "{} '{}' uses the absolute config_db path \"{}\". The class then only works at that one place in one test's \
hierarchy. Use 'this' as the context and a path relative to it.",
                    role_name(r),
                    cls.name,
                    path
                )
            } else {
                format!(
                    "config_db {}() in '{}' combines a non-null context with the absolute path \"{}\". UVM appends the path to the \
context's full name, so it can never match. Pass null as the context, or make the path relative.",
                    if is_set { "set" } else { "get" },
                    cls.name,
                    path
                )
            };
            out.add("UVM-RUS-004", "AbsoluteConfigDbPath", Severity::Warning, msg, c.loc_of(call));
        }
    }
}
