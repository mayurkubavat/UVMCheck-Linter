//! Group A pattern rules: callbacks, register adapters, check failures,
//! singletons, extension points and interface assertions (MG-5, MG-7,
//! MG-8).

use std::collections::HashSet;

use dv_model::{Class, Design};
use sv_syntax::{Child, NodeId};

use crate::g_meth_body::{bodies, calls_in, if_parts, integer_literal, is_report_guard, is_stmt};
use crate::g_meth_model::overrides_base;
use crate::g_methodology::{class_methods, derives, is_library, role, role_name, uvm_base_name, user_classes, Ctx, Index, Role};
use crate::syntax::Cst;
use crate::Check::Mixed;
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "UVM-PAT-001", name: "CallbackNotRegistered", check: Mixed(callback_not_registered) },
        Rule { id: "RAL-ADP-001", name: "IncompleteRegAdapter", check: Mixed(incomplete_reg_adapter) },
        Rule { id: "UVM-REP-007", name: "CheckFailureAsWarning", check: Mixed(check_failure_as_warning) },
        Rule { id: "UVM-REP-008", name: "FatalInCheckerPath", check: Mixed(fatal_in_checker_path) },
        Rule { id: "UVM-PAT-002", name: "HandRolledSingleton", check: Mixed(hand_rolled_singleton) },
        Rule { id: "UVM-PAT-003", name: "NoExtensionPoint", check: Mixed(no_extension_point) },
        Rule { id: "SV-DBC-001", name: "InterfaceWithoutAssertions", check: Mixed(interface_without_assertions) },
    ]
}

// ---- UVM-PAT-001 ----

/// The type arguments of `name#(A, B)` written in a data type or name.
fn type_args(c: &Cst, n: NodeId) -> Vec<String> {
    let Some(pl) = c.subtree(n).into_iter().find(|&x| c.kind(x) == ParamList) else { return Vec::new() };
    let Some(al) = c.tree.child(pl, ArgList) else { return Vec::new() };
    c.tree
        .children(al)
        .filter(|&a| c.kind(a) == Arg)
        .filter_map(|a| c.tree.children(a).next().map(|e| c.tree.bare_text(e)))
        .collect()
}

fn same_or_derived(d: &Design, a: &Class, b: &Class) -> bool {
    a.id == b.id || derives(d, a.id, b.id)
}

fn callback_not_registered(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    // `uvm_register_cb(T, CB): static m_register_cb_<CB> = uvm_callbacks#(T, CB)::m_register_pair().
    let mut regs: Vec<(&Class, &Class)> = Vec::new();
    for cls in user_classes(d) {
        let Some((c, decl)) = ix.decl(cls.loc) else { continue };
        if c.kind(decl) != ClassDecl {
            continue;
        }
        let ctx = Ctx { d, cls, m: None, td: Some(&ix.typedefs) };
        for dd in c.items(decl).into_iter().filter(|&x| c.kind(x) == DataDecl) {
            for dcl in c.tree.children(dd).filter(|&x| c.kind(x) == Declarator) {
                let Some(name) = c.tree.child(dcl, Name) else { continue };
                if !c.tree.bare_text(name).starts_with("m_register_cb_") {
                    continue;
                }
                let Some(init) = c.tok(dcl, "=").and_then(|eq| c.node_after(dcl, eq)) else { continue };
                let init = c.strip(init);
                if c.kind(init) != CallExpr {
                    continue;
                }
                let Some(callee) = c.tree.children(init).next() else { continue };
                if c.kind(callee) != ScopedExpr {
                    continue;
                }
                let Some(owner) = c.tree.children(callee).next() else { continue };
                if c.kind(owner) != NameExpr || !c.tree.bare_text(owner).starts_with("uvm_callbacks#") {
                    continue;
                }
                let args = type_args(c, owner);
                let (Some(t), Some(cb)) = (
                    args.first().and_then(|a| ctx.class_of_type(a)),
                    args.get(1).and_then(|a| ctx.class_of_type(a)),
                ) else {
                    continue;
                };
                regs.push((t, cb));
            }
        }
    }
    for (cls, m, c, body) in bodies(d, &ix) {
        let ctx = Ctx { d, cls, m: Some(m), td: Some(&ix.typedefs) };
        for dd in c.subtree(body).into_iter().filter(|&x| c.kind(x) == DataDecl) {
            let Some(ty) = c.tree.child(dd, DataType) else { continue };
            if c.tree.children(ty).next().is_none() && !c.tree.bare_text(ty).starts_with("uvm_callback_iter") {
                continue;
            }
            let text = c.tree.bare_text(ty);
            if text.split('#').next() != Some("uvm_callback_iter") {
                continue;
            }
            let args = type_args(c, ty);
            let (Some(t), Some(cb)) = (
                args.first().and_then(|a| ctx.class_of_type(a)),
                args.get(1).and_then(|a| ctx.class_of_type(a)),
            ) else {
                continue;
            };
            if regs.iter().any(|(rt, rcb)| same_or_derived(d, t, rt) && same_or_derived(d, cb, rcb)) {
                continue;
            }
            for dcl in c.tree.children(dd).filter(|&x| c.kind(x) == Declarator) {
                let Some(name) = c.tree.child(dcl, Name) else { continue };
                out.add(
                    "UVM-PAT-001",
                    "CallbackNotRegistered",
                    Severity::Warning,
                    format!(
                        "'{}' runs callbacks of type '{}' for '{}', but no class registers the pair with `uvm_register_cb({}, {}). uvm_callbacks::add() rejects unregistered pairs, so these callbacks are \
never called. Add `uvm_register_cb({}, {}) to '{}'.",
                        cls.name, cb.name, t.name, t.name, cb.name, t.name, cb.name, t.name
                    ),
                    c.loc_of(name),
                );
            }
        }
    }
}

// ---- RAL-ADP-001 ----

#[derive(Default)]
struct ArgUse {
    written: Vec<String>,
    read: Vec<String>,
    whole: bool,
}

fn is_arg(c: &Cst, e: NodeId, arg: &str) -> bool {
    let e = c.strip(e);
    c.kind(e) == NameExpr && c.tree.children(e).next().is_none() && c.tree.bare_text(e) == arg
}

fn arg_uses(c: &Cst, n: NodeId, arg: &str, u: &mut ArgUse) {
    if let Some(lhs) = crate::g_meth_body2::assignment_lhs(c, n) {
        let l = c.strip(lhs);
        if c.kind(l) == MemberExpr && c.tree.children(l).next().is_some_and(|v| is_arg(c, v, arg)) {
            u.written.push(c.last_text(l).to_string());
            if let Some(rhs) = c.tree.children(n).nth(1) {
                arg_uses(c, rhs, arg, u);
            }
            return;
        }
        if is_arg(c, l, arg) {
            u.whole = true;
        }
    }
    match c.kind(n) {
        MemberExpr => {
            if c.tree.children(n).next().is_some_and(|v| is_arg(c, v, arg)) {
                u.read.push(c.last_text(n).to_string());
            }
        }
        CallExpr => {
            for a in c.call_args(n) {
                if let Some(e) = c.arg_expr(a) {
                    if is_arg(c, e, arg) {
                        u.whole = true;
                    }
                }
            }
        }
        _ => {}
    }
    for k in c.node_kids(n) {
        arg_uses(c, k, arg, u);
    }
}

fn incomplete_reg_adapter(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for (cls, m, c, body) in bodies(d, &ix) {
        if uvm_base_name(d, cls) != "uvm_reg_adapter" {
            continue;
        }
        let why = if m.name == "bus2reg" && m.arguments.len() >= 2 {
            let mut u = ArgUse::default();
            arg_uses(c, body, &m.arguments[1].name, &mut u);
            if u.whole || u.written.iter().any(|w| w == "status") {
                continue;
            }
            "bus2reg() never sets rw.status, so every register access is reported as UVM_IS_OK even when the bus returned an error. Copy the bus response \
status into rw.status."
        } else if m.name == "reg2bus" && !m.arguments.is_empty() {
            let mut u = ArgUse::default();
            arg_uses(c, body, &m.arguments[0].name, &mut u);
            if u.whole || u.read.iter().any(|w| w == "kind") {
                continue;
            }
            "reg2bus() never reads rw.kind, so reads and writes become the same bus transaction. Set the item's direction from rw.kind."
        } else {
            continue;
        };
        out.add(
            "RAL-ADP-001",
            "IncompleteRegAdapter",
            Severity::Warning,
            format!("Register adapter '{}': {why}", cls.name),
            m.loc,
        );
    }
}

// ---- UVM-REP-007 / UVM-REP-008 ----

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Level {
    None,
    Info,
    Warning,
    Error,
    Fatal,
}

/// `swe::strongestReport`.
pub(crate) fn strongest_report(c: &Cst, n: NodeId) -> Level {
    calls_in(c, n)
        .into_iter()
        .map(|k| match c.callee_name(k) {
            "$fatal" | "uvm_report_fatal" => Level::Fatal,
            "$error" | "uvm_report_error" => Level::Error,
            "$warning" | "uvm_report_warning" => Level::Warning,
            "$info" | "uvm_report_info" => Level::Info,
            _ => Level::None,
        })
        .max()
        .unwrap_or(Level::None)
}

fn has_compare_call(c: &Cst, e: NodeId) -> bool {
    calls_in(c, e).into_iter().any(|k| c.callee_name(k) == "compare")
}

fn is_zero(c: &Cst, e: NodeId) -> bool {
    let e = c.strip(e);
    if c.kind(e) != Literal {
        return false;
    }
    let t = c.expr_text(e);
    (t == "0" || t == "1'b0" || t == "'0") && (integer_literal(c, e).is_some() || t == "'0")
}

fn names_expected(t: &str) -> bool {
    t.contains("exp") || t.contains("ref") || t.contains("pred")
}

fn names_actual(t: &str) -> bool {
    ["act", "obs", "got", "rcv", "dut"].iter().any(|w| t.contains(w))
}

/// Which branch runs when the check fails: +1 then, -1 else, 0 not a check.
fn failing_branch(c: &Cst, cond: NodeId, what: &mut String) -> i32 {
    let cond = c.strip(cond);
    match c.kind(cond) {
        PrefixExpr => {
            let op = c.toks(cond).next().map(|t| c.text_of(t)).unwrap_or("");
            if op == "!" && c.tree.children(cond).next().is_some_and(|x| has_compare_call(c, x)) {
                *what = "compare()".into();
                return 1;
            }
            0
        }
        CallExpr => {
            if has_compare_call(c, cond) {
                *what = "compare()".into();
                return -1;
            }
            0
        }
        BinaryExpr => {
            let op = c.toks(cond).next().map(|t| c.text_of(t)).unwrap_or("");
            let eq = op == "==" || op == "===";
            let ne = op == "!=" || op == "!==";
            if !eq && !ne {
                return 0;
            }
            let mut sides = c.tree.children(cond);
            let (Some(l), Some(r)) = (sides.next(), sides.next()) else { return 0 };
            let (cl, cr) = (has_compare_call(c, l), has_compare_call(c, r));
            if (cl && is_zero(c, r)) || (cr && is_zero(c, l)) {
                *what = "compare()".into();
                return if eq { 1 } else { -1 };
            }
            let (lt, rt) = (c.expr_text(l).to_ascii_lowercase(), c.expr_text(r).to_ascii_lowercase());
            if (names_expected(&lt) && names_actual(&rt)) || (names_actual(&lt) && names_expected(&rt)) {
                *what = format!("'{}'", c.expr_text(cond));
                return if ne { 1 } else { -1 };
            }
            0
        }
        _ => 0,
    }
}

/// `meth::failureBranches`: (branch, condition, what).
fn failure_branches(c: &Cst, body: NodeId) -> Vec<(NodeId, NodeId, String)> {
    let mut v = Vec::new();
    for n in c.subtree(body) {
        if c.kind(n) != IfStmt || is_report_guard(c, n) {
            continue;
        }
        let (Some(cond), then, other) = if_parts(c, n) else { continue };
        let mut what = String::new();
        let side = failing_branch(c, cond, &mut what);
        let branch = match side {
            1 => then,
            -1 => other,
            _ => None,
        };
        if let Some(b) = branch {
            v.push((b, cond, what));
        }
    }
    v
}

fn check_failure_as_warning(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for (cls, m, c, body) in bodies(d, &ix) {
        for (b, cond, what) in failure_branches(c, body) {
            let lvl = strongest_report(c, b);
            if lvl != Level::Warning && lvl != Level::Info {
                continue;
            }
            out.add(
                "UVM-REP-007",
                "CheckFailureAsWarning",
                Severity::Warning,
                format!(
                    "A failed check ({what}) in '{}::{}' is reported with `uvm_{}. Warnings do not fail the test, so the mismatch goes unnoticed in \
regression; report it with `uvm_error.",
                    cls.name,
                    m.name,
                    if lvl == Level::Warning { "warning" } else { "info" }
                ),
                c.loc_of(cond),
            );
        }
    }
}

fn fatal_in_checker_path(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for (cls, m, c, body) in bodies(d, &ix) {
        let r = role(d, cls);
        if r != Role::Scoreboard && r != Role::Subscriber {
            continue;
        }
        for (b, cond, what) in failure_branches(c, body) {
            if strongest_report(c, b) != Level::Fatal {
                continue;
            }
            out.add(
                "UVM-REP-008",
                "FatalInCheckerPath",
                Severity::Warning,
                format!(
                    "{} '{}' stops the run with `uvm_fatal when a check ({what}) fails in {}(). One mismatch then hides all later ones and \
skips the report phase summary; report it with `uvm_error.",
                    role_name(r),
                    cls.name,
                    m.name
                ),
                c.loc_of(cond),
            );
        }
    }
}

// ---- UVM-PAT-002 ----

fn singleton_finding(cls: &Class, var: &str, loc: dv_model::Loc, out: &mut Sink) {
    out.add(
        "UVM-PAT-002",
        "HandRolledSingleton",
        Severity::Warning,
        format!(
            "Class '{}' keeps a static instance of itself in '{var}'. Every env and test shares that one object, and a test \
cannot substitute another. Create it in the test or env and pass it through uvm_config_db or the resource database.",
            cls.name
        ),
        loc,
    );
}

fn hand_rolled_singleton(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for cls in user_classes(d) {
        for p in &cls.properties {
            if p.is_static && !p.is_const && p.type_class == Some(cls.id) {
                singleton_finding(cls, &p.name, p.loc, out);
            }
        }
        // Static locals declared at the top of a method body (not extern ones).
        for m in class_methods(cls).filter(|m| !m.is_extern) {
            let Some((c, body)) = ix.body(m) else { continue };
            let ctx = Ctx { d, cls, m: Some(m), td: Some(&ix.typedefs) };
            for dd in c.items(body).into_iter().filter(|&x| c.kind(x) == DataDecl) {
                if c.tok(dd, "static").is_none() || c.tok(dd, "const").is_some() {
                    continue;
                }
                let Some(ty) = c.tree.child(dd, DataType) else { continue };
                if ctx.class_of_type(&c.tree.bare_text(ty)).map(|k| k.id) != Some(cls.id) {
                    continue;
                }
                for dcl in c.tree.children(dd).filter(|&x| c.kind(x) == Declarator) {
                    if c.tree.child(dcl, Dim).is_some() {
                        continue;
                    }
                    let Some(name) = c.tree.child(dcl, Name) else { continue };
                    singleton_finding(cls, &c.tree.bare_text(name), c.loc_of(name), out);
                }
            }
        }
    }
}

// ---- UVM-PAT-003 ----

fn is_scaffolding(c: &Cst, s: NodeId) -> bool {
    match c.kind(s) {
        DataDecl => c
            .tree
            .children(s)
            .filter(|&x| c.kind(x) == Declarator)
            .filter_map(|x| c.tree.child(x, Name))
            .any(|x| c.tree.bare_text(x).starts_with("_local_report_object_")),
        ExprStmt => c
            .items(s)
            .first()
            .and_then(|&e| crate::g_meth_body2::assignment_lhs(c, e))
            .is_some_and(|l| c.tree.bare_text(l).starts_with("_local_report_object_")),
        _ => false,
    }
}

/// `swe::countStatements`: statements, not blocks; a `uvm_* macro counts
/// as its report call.
pub(crate) fn count_statements(c: &Cst, n: NodeId) -> usize {
    let mut count = 0;
    count_into(c, n, &mut count);
    count
}

fn count_into(c: &Cst, n: NodeId, count: &mut usize) {
    if is_scaffolding(c, n) {
        return;
    }
    match c.kind(n) {
        IfStmt if is_report_guard(c, n) => {
            if let (_, Some(t), _) = if_parts(c, n) {
                count_into(c, t, count);
            }
            return;
        }
        DataDecl => {
            *count += c.tree.children(n).filter(|&x| c.kind(x) == Declarator).count().max(1);
            return;
        }
        SeqBlock | ParBlock | EmptyStmt | Body => {}
        _ if is_stmt(c, n) => *count += 1,
        _ => {}
    }
    let in_loop = c.kind(n) == LoopStmt;
    for k in c.node_kids(n) {
        match c.kind(k) {
            ElseClause => {
                for e in c.node_kids(k) {
                    count_into(c, e, count);
                }
            }
            DataDecl if !in_loop => count_into(c, k, count),
            _ if is_stmt(c, k) => count_into(c, k, count),
            _ => {}
        }
    }
}

fn no_extension_point(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    const MIN: usize = 25;
    for cls in user_classes(d) {
        let r = role(d, cls);
        if r != Role::Driver && r != Role::Monitor {
            continue;
        }
        let Some(run) = class_methods(cls).filter(|m| m.name == "run_phase").last() else { continue };
        let Some((c, body)) = ix.body(run) else { continue };
        let statements = count_statements(c, body);
        if statements < MIN {
            continue;
        }
        let mut hooks = false;
        for k in std::iter::once(cls).chain(d.ancestors(cls.id)) {
            if is_library(d, k) {
                break;
            }
            hooks |= class_methods(k).any(|m| m.is_virtual && !overrides_base(d, k, &m.name) && m.name != "new");
            if let Some((kc, kn)) = ix.decl(k.loc) {
                let text = kc.text(kn);
                hooks |= text.contains("uvm_callback") || text.contains("uvm_do_callbacks") || text.contains("uvm_register_cb");
            }
        }
        if hooks {
            continue;
        }
        out.add(
            "UVM-PAT-003",
            "NoExtensionPoint",
            Severity::Info,
            format!(
                "{} '{}' does all its work in a {statements}-statement run_phase and offers no virtual hook methods or callbacks. A test or a \
derived agent cannot change one step (error injection, timing, a protocol variant) without copying the whole run_phase. Split the steps into virtual methods \
(drive_item, wait_for_reset, collect_transfer) or add a uvm_callback.",
                if r == Role::Driver { "Driver" } else { "Monitor" },
                cls.name
            ),
            run.loc,
        );
    }
}

// ---- SV-DBC-001 ----

fn interface_without_assertions(d: &Design, csts: &[Cst], out: &mut Sink) {
    let mut bound = HashSet::new();
    let mut candidates = Vec::new();
    for c in csts {
        for n in c.nodes(BindDirective) {
            // bind <target> <module> <instance>: the target is the first name.
            if let Some(t) = c.tree.children(n).next() {
                bound.insert(c.trimmed(t));
            }
        }
        for n in c.nodes(InterfaceDecl) {
            let (mut protocol, mut assertions) = (false, false);
            for x in c.subtree(n) {
                match c.kind(x) {
                    ClockingDecl | ModportDecl => protocol = true,
                    ConcurrentAssert | PropertyDecl => assertions = true,
                    AssertStmt => assertions = true,
                    Instantiation if is_checker_inst(csts, c, x) => assertions = true,
                    _ => {}
                }
            }
            if !protocol || assertions {
                continue;
            }
            let Some(name) = c.tree.child(n, Name) else { continue };
            let Some((_, last)) = c.tree.token_bounds(name) else { continue };
            candidates.push((c.tree.bare_text(name), c.loc(last)));
        }
    }
    for (name, loc) in candidates {
        let f = d.file_name(loc).to_string();
        let f = if f.is_empty() { String::new() } else { f };
        let base = f.rsplit(['/', '\\']).next().unwrap_or("");
        if bound.contains(&name) || base.starts_with("uvm_") {
            continue;
        }
        out.add(
            "SV-DBC-001",
            "InterfaceWithoutAssertions",
            Severity::Info,
            format!(
                "Interface '{name}' defines a protocol (clocking blocks or modports) but holds no assertions. Every testbench that reuses it re-checks the protocol by hand, or not at \
all. Write the protocol's rules into it as assert property statements, or bind a checker to it."
            ),
            loc,
        );
    }
}

/// An instantiation of a checker declared in one of the trees.
fn is_checker_inst(csts: &[Cst], c: &Cst, n: NodeId) -> bool {
    let Some(ty) = c.kids(n).iter().find_map(|k| match *k {
        Child::Node(x) => Some(x),
        Child::Token(_) => None,
    }) else {
        return false;
    };
    let name = c.tree.bare_text(ty);
    csts.iter().any(|t| {
        t.nodes(CheckerDecl)
            .into_iter()
            .any(|k| t.tree.child(k, Name).is_some_and(|x| t.tree.bare_text(x) == name))
    })
}
