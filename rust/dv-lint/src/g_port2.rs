//! Rules ported after slice 5, from their C++ twins with the same ids,
//! names, severities and messages.

use std::collections::{HashMap, HashSet};

use dv_model::{Class, Design};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

use crate::g_meth_body::{bodies, calls_in};
use crate::g_methodology::{role, Index, Role};
use crate::syntax::Cst;
use crate::g_design_class::{is_report_guard, is_report_scaffolding, report_level, Level};
use crate::Check::{Mixed, Syntax};
use crate::{Rule, Severity, Sink};

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "UVM-REP-006", name: "BlanketErrorDemotion", check: Mixed(blanket_error_demotion) },
        Rule { id: "UVM-TST-001", name: "CheckerCannotFail", check: Mixed(checker_cannot_fail) },
        Rule { id: "SV-ERR-001", name: "FailureNotReported", check: Syntax(failure_not_reported) },
    ]
}

// ---- UVM-REP-006 ----

fn is_error_severity(a: &str) -> bool {
    a.contains("UVM_ERROR") || a.contains("UVM_FATAL")
}

fn is_weak_severity(a: &str) -> bool {
    a.contains("UVM_INFO") || a.contains("UVM_WARNING")
}

/// An action that neither counts the error nor stops the run.
fn drops_action(a: &str) -> bool {
    a.contains("UVM_NO_ACTION") || (!a.contains("UVM_COUNT") && !a.contains("UVM_EXIT") && !a.contains("UVM_STOP"))
}

fn blanket_error_demotion(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for (cls, m, c, body) in bodies(d, &ix) {
        let calls = calls_in(c, body);
        let arg_text = |call, i: usize| {
            let args = c.call_args(call);
            args.get(i).and_then(|a| c.arg_expr(*a)).map(|e| c.expr_text(e))
        };
        if role(d, cls) == Role::Catcher && m.name == "catch" {
            let mut filters = false;
            let mut demotion = None;
            for &call in &calls {
                let n = c.callee_name(call);
                if matches!(n, "get_id" | "get_message" | "get_fname" | "get_context" | "get_client") {
                    filters = true;
                }
                if demotion.is_some() || c.call_args(call).is_empty() {
                    continue;
                }
                let Some(a) = arg_text(call, 0) else { continue };
                if (n == "set_severity" && is_weak_severity(&a)) || (n == "set_action" && drops_action(&a)) {
                    demotion = Some(call);
                }
            }
            let caught = c.subtree(body).into_iter().any(|r| {
                c.kind(r) == ReturnStmt && c.tree.children(r).next().is_some_and(|e| c.expr_text(e) == "CAUGHT")
            });
            if !filters && (demotion.is_some() || caught) {
                out.add(
                    "UVM-REP-006",
                    "BlanketErrorDemotion",
                    Severity::Error,
                    format!(
                        "Report catcher '{}' {} without checking get_id() or get_message(), so every matching error in the run is \
hidden, including ones nobody has seen yet. Filter on the specific ID or message.",
                        cls.name,
                        if demotion.is_some() { "demotes reports" } else { "returns CAUGHT" }
                    ),
                    demotion.map(|x| c.loc_of(x)).unwrap_or(m.loc),
                );
            }
        }
        for &call in &calls {
            let n = c.callee_name(call);
            let (Some(a0), Some(a1)) = (arg_text(call, 0), arg_text(call, 1)) else { continue };
            let bad = (n == "set_report_severity_override" && is_error_severity(&a0) && is_weak_severity(&a1))
                || (matches!(n, "set_report_severity_action" | "set_report_severity_action_hier")
                    && is_error_severity(&a0)
                    && drops_action(&a1));
            if bad {
                out.add(
                    "UVM-REP-006",
                    "BlanketErrorDemotion",
                    Severity::Error,
                    format!(
                        "{n}({a0}, {a1}) changes how every {a0} is handled, for all report IDs. Errors stop counting and the test can pass \
with real failures. Use the *_id_* variant for the specific ID instead."
                    ),
                    c.loc_of(call),
                );
            }
        }
    }
}

// ---- UVM-TST-001 ----

fn looks_like_checker(d: &Design, cls: &Class) -> bool {
    let r = role(d, cls);
    if r == Role::Scoreboard {
        return true;
    }
    // Names only count for components that receive transactions.
    if r != Role::Component && r != Role::Subscriber {
        return false;
    }
    let n = cls.name.to_lowercase();
    n.contains("scoreboard") || n.contains("checker") || n.ends_with("_scb") || n.contains("_scb_") || n.ends_with("_sb")
}

/// A routine written in the library (uvm_pkg, a uvm_* class, the stub).
fn routine_in_library(d: &Design, c: &Cst, n: NodeId) -> bool {
    let f = d.file_name(c.loc_of(n));
    if f.contains("uvm_mock.svh") || f.contains("uvm_pkg.sv") {
        return true;
    }
    std::iter::successors(c.parent(n), |&a| c.parent(a)).any(|a| {
        matches!(c.kind(a), ClassDecl | PackageDecl)
            && c.tree.child(a, sv_syntax::SyntaxKind::Name).is_some_and(|nm| c.tree.bare_text(nm).starts_with("uvm_"))
    })
}

/// The routine can report a failure: an assertion or an error-level
/// report in it, or in a routine it calls (three calls deep).
fn routine_can_fail(d: &Design, c: &Cst, n: NodeId, depth: u32, memo: &mut HashMap<(usize, u32), bool>) -> bool {
    if let Some(&v) = memo.get(&(c.index(), n.0)) {
        return v;
    }
    memo.insert((c.index(), n.0), false);
    let mut found = false;
    let mut callees = Vec::new();
    for x in c.subtree(n) {
        match c.kind(x) {
            AssertStmt | ConcurrentAssert => found = true,
            CallExpr => {
                if crate::g_design_class::report_level(c, x) >= crate::g_design_class::Level::Error {
                    found = true;
                }
                if let Some(r) = crate::g_c_support::callee_decl(c, x) {
                    callees.push(r);
                }
            }
            _ => {}
        }
        if found {
            break;
        }
    }
    if !found && depth < 3 {
        for r in callees {
            let r = out_of_block_body(c, r).unwrap_or(r);
            if c.kind(r) == MethodPrototype || routine_in_library(d, c, r) {
                continue;
            }
            if routine_can_fail(d, c, r, depth + 1, memo) {
                found = true;
                break;
            }
        }
    }
    memo.insert((c.index(), n.0), found);
    found
}

/// The body of an `extern` method: `function T cls::name(...)`.
fn out_of_block_body(c: &Cst, proto: NodeId) -> Option<NodeId> {
    if c.kind(proto) != MethodPrototype {
        return None;
    }
    let cls = std::iter::successors(c.parent(proto), |&a| c.parent(a)).find(|&a| c.kind(a) == ClassDecl)?;
    let cname = c.tree.bare_text(c.tree.child(cls, sv_syntax::SyntaxKind::Name)?);
    let name = crate::g_c_support::decl_name(c, proto);
    let want = format!("{cname}::{}", name.rsplit("::").next().unwrap_or(&name));
    c.nodes_of(&[FunctionDecl, TaskDecl]).into_iter().find(|&f| crate::g_c_support::decl_name(c, f) == want)
}

fn class_can_fail(d: &Design, ix: &Index, cls: &Class, memo: &mut HashMap<(usize, u32), bool>) -> bool {
    let mut cur = Some(cls);
    let mut guard = 0;
    while let Some(k) = cur {
        guard += 1;
        if guard > 32 {
            break;
        }
        // The library comparators report mismatches themselves.
        if k.name.contains("comparator") {
            return true;
        }
        if !crate::g_methodology::is_library(d, k) {
            for m in k.methods.iter().filter(|m| !m.is_builtin && !m.is_pure) {
                let body = ix.body(m).or_else(|| {
                    // An extern method's body, out of the class.
                    let want = format!("{}::{}", k.name, m.name);
                    ix.csts.iter().find_map(|c| {
                        c.nodes_of(&[FunctionDecl, TaskDecl])
                            .into_iter()
                            .find(|&f| crate::g_c_support::decl_name(c, f) == want)
                            .map(|f| (c, f))
                    })
                });
                if let Some((c, body)) = body {
                    if routine_can_fail(d, c, body, 0, memo) {
                        return true;
                    }
                }
            }
            // A generic class has no methods in the model: its own routines
            // and out-of-block bodies from the syntax.
            if k.methods.is_empty() {
                if let Some((c, decl)) = ix.decl(k.loc).filter(|(c, n)| c.kind(*n) == ClassDecl) {
                    let prefix = format!("{}::", k.name);
                    let own = c.subtree(decl).into_iter().filter(|&f| matches!(c.kind(f), FunctionDecl | TaskDecl));
                    let outside = c
                        .nodes_of(&[FunctionDecl, TaskDecl])
                        .into_iter()
                        .filter(|&f| crate::g_c_support::decl_name(c, f).starts_with(&prefix));
                    let routines: Vec<NodeId> = own.chain(outside).collect();
                    if routines.into_iter().any(|f| routine_can_fail(d, c, f, 0, memo)) {
                        return true;
                    }
                }
            }
            for p in &k.properties {
                let pc = p.type_class.map(|i| d.class(i).name.to_lowercase());
                if pc.is_some_and(|n| n.contains("comparator")) {
                    return true;
                }
            }
        }
        cur = k.base.map(|b| d.class(b));
    }
    false
}

fn checker_cannot_fail(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    let classes: Vec<&Class> = crate::g_methodology::user_classes(d).collect();
    // Base classes are judged through the classes that extend them.
    let extended: HashSet<String> = classes.iter().filter_map(|k| k.base.map(|b| d.class(b).name.clone())).collect();
    let mut memo = HashMap::new();
    for cls in classes {
        if cls.is_virtual || cls.is_interface || !looks_like_checker(d, cls) || extended.contains(&cls.name) {
            continue;
        }
        if class_can_fail(d, &ix, cls, &mut memo) {
            continue;
        }
        out.add(
            "UVM-TST-001",
            "CheckerCannotFail",
            Severity::Error,
            format!(
                "Checker '{}' never reports an error: it has no `uvm_error/`uvm_fatal, assertion or comparator. Every test using it \
passes regardless of what the DUT does.",
                cls.name
            ),
            cls.loc,
        );
    }
}

// ---- SV-ERR-001 ----

#[derive(Clone, Copy, PartialEq)]
enum Status {
    Randomize,
    Cast,
    ConfigGet,
    TryOp,
    Compare,
}

fn status_name(s: Status) -> &'static str {
    match s {
        Status::Randomize => "randomize()",
        Status::Cast => "$cast()",
        Status::ConfigGet => "uvm_config_db::get()",
        Status::TryOp => "try_* call",
        Status::Compare => "compare()",
    }
}

fn classify(c: &Cst, e: NodeId) -> Option<Status> {
    let e = crate::g_design::strip_parens(c, e);
    let n = crate::g_design::call_name(c, e)?;
    match n {
        "randomize" => Some(Status::Randomize),
        "$cast" => Some(Status::Cast),
        "try_get" | "try_put" | "try_peek" => Some(Status::TryOp),
        "compare" => Some(Status::Compare),
        "get" if c.expr_text(e).contains("uvm_config_db") => Some(Status::ConfigGet),
        _ => None,
    }
}

/// The status call a condition tests for failure (`!call`, `call == 0`).
fn failure_test(c: &Cst, cond: NodeId) -> Option<Status> {
    let e = crate::g_design::strip_parens(c, cond);
    let kids: Vec<NodeId> = c.tree.children(e).collect();
    let op = c.toks(e).next().map(|t| c.text_of(t)).unwrap_or("");
    match c.kind(e) {
        PrefixExpr if op == "!" => classify(c, *kids.first()?),
        BinaryExpr if op == "==" || op == "===" => {
            let r = c.expr_text(*kids.get(1)?);
            if matches!(r.as_str(), "0" | "1'b0" | "'0") {
                classify(c, *kids.first()?)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// The statement does nothing but report below error severity (or nothing).
fn only_weak_reports(c: &Cst, st: NodeId, strongest: &mut Level) -> bool {
    match c.kind(st) {
        EmptyStmt => true,
        SeqBlock => c.items(st).into_iter().all(|x| only_weak_reports(c, x, strongest)),
        IfStmt => {
            // `uvm_* macros expand to a report guard around uvm_report_*(...).
            let (_, then, other) = crate::g_meth_body::if_parts(c, st);
            if other.is_some() || c.tree.child(st, ElseClause).is_some() || !is_report_guard(c, st) {
                return false;
            }
            then.is_none_or(|t| only_weak_reports(c, t, strongest))
        }
        DataDecl => is_report_scaffolding(c, st),
        ExprStmt => {
            if is_report_scaffolding(c, st) {
                return true;
            }
            let Some(e) = c.tree.children(st).next().map(|e| crate::g_design::strip_parens(c, e)) else { return false };
            if c.kind(e) != CallExpr {
                return false;
            }
            let level = report_level(c, e);
            let n = crate::g_design::call_name(c, e).unwrap_or("");
            if level == Level::None && n != "$display" && n != "$write" {
                return false;
            }
            if level >= Level::Error {
                return false;
            }
            if level > *strongest {
                *strongest = level;
            }
            true
        }
        _ => false,
    }
}

fn failure_not_reported(c: &Cst, out: &mut Sink) {
    for st in c.nodes_of(&[IfStmt, AssertStmt]) {
        let times = crate::g_sva_cov::user_code_times(c, st);
        if times == 0 {
            continue;
        }
        let found = if c.kind(st) == IfStmt {
            let (Some(cond), then, other) = crate::g_meth_body::if_parts(c, st) else { continue };
            if c.kind(cond) == MatchExpr {
                continue;
            }
            match failure_test(c, cond) {
                Some(s) => then.map(|t| (s, t)),
                None => classify(c, cond).zip(other),
            }
        } else {
            // `assert (call) else ...`
            if c.toks(st).next().is_none_or(|t| c.text_of(t) != "assert") {
                continue;
            }
            let cond = c.tree.children(st).next();
            let other = c.tree.child(st, ElseClause).and_then(|e| c.items(e).into_iter().next());
            cond.and_then(|x| classify(c, x)).zip(other)
        };
        let Some((s, branch)) = found else { continue };
        let mut strongest = Level::None;
        if !only_weak_reports(c, branch, &mut strongest) {
            continue;
        }
        // A missing optional config_db entry reported as a warning is a common, valid choice.
        if s == Status::ConfigGet && strongest == Level::Warning {
            continue;
        }
        let how = match strongest {
            Level::None => "is silently ignored",
            Level::Info => "is only reported with `uvm_info",
            _ => "is only reported as a warning",
        };
        for _ in 0..times {
            out.add(
                "SV-ERR-001",
                "FailureNotReported",
                Severity::Error,
                format!(
                    "Failure of {} {how}, so the test still passes after detecting it. Report it with `uvm_error or `uvm_fatal, or handle the \
failure explicitly.",
                    status_name(s)
                ),
                c.loc_of(st),
            );
        }
    }
}
