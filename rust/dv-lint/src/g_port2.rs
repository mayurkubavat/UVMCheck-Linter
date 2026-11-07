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
        Rule { id: "SV-ERR-002", name: "UseAfterFailedNullCheck", check: Syntax(use_after_failed_null_check) },
        Rule { id: "SV-ASC-001", name: "AssociativeArrayMutatingRead", check: Syntax(assoc_mutating_read) },
        Rule { id: "SV-CB-001", name: "ClockingBlockOutputReadSkew", check: Syntax(clocking_output_read) },
        Rule { id: "SV-CMP-001", name: "CompilerError", check: Syntax(compiler_errors) },
        Rule { id: "SV-DED-001", name: "UnusedVariable", check: Syntax(unused_variable) },
        Rule { id: "SV-DED-012", name: "UnreferencedParameter", check: Syntax(unreferenced_parameter) },
        Rule { id: "SV-DED-013", name: "UnusedRoutineArgument", check: Syntax(unused_routine_argument) },
        Rule { id: "SV-DED-014", name: "UnusedLocalMethod", check: Syntax(unused_local_method) },
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
        let cond = c.tree.children(st).next();
        if cond.is_some_and(|x| !crate::g_random_sema::ast_ok(c, x)) {
            continue;
        }
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

// ---- SV-ERR-002 ----

use crate::g_c_support::{call_parts, class_member, expr_type, find_class, kids, lookup, user_routines, Decl};
use crate::g_concurrency::{assign_lhs, call_args, items, start, PathHooks, Walker};

/// What a type name names: a class (its declaration, when in the tree), or
/// another handle type (virtual interface, built-in class). Follows type
/// parameters (their default) and typedefs.
enum HandleTy {
    Class(NodeId),
    Other,
}

fn handle_type(c: &Cst, dtype: NodeId, ctx: Option<NodeId>, depth: u32) -> Option<HandleTy> {
    let t = c.text_of(c.first_tok(dtype)?);
    if t == "virtual" || matches!(t, "mailbox" | "semaphore" | "process") {
        return Some(HandleTy::Other);
    }
    if let Some(k) = find_class(c, t, dtype) {
        return Some(HandleTy::Class(k));
    }
    if depth > 8 {
        return None;
    }
    if let Some(d) = lookup(c, dtype, t) {
        if d.kind != crate::g_c_support::DeclKind::Parameter || c.kind(d.node) != ParamDecl {
            return None;
        }
        if let Some((arg, x)) = ctx.and_then(|e| param_override(c, d.node, t, e)) {
            return handle_type(c, arg, Some(x), depth + 1);
        }
        // `type T = cls`: the default type.
        let toks = c.tree.token_range(d.node);
        let eq = *toks.iter().find(|&&x| c.text_of(x) == "=")?;
        let dt = kids(c, d.node).into_iter().find(|&k| c.kind(k) == DataType && c.first_tok(k).is_some_and(|f| f > eq))?;
        return handle_type(c, dt, ctx, depth + 1);
    }
    let td = c.nodes(TypedefDecl).into_iter().find(|&td| c.tree.child(td, Name).is_some_and(|x| c.tree.bare_text(x) == t))?;
    let dt = c.tree.child(td, DataType)?;
    handle_type(c, dt, ctx, depth + 1)
}

/// The type a class below `ctx`'s chain passes for type parameter `name`
/// (declared at `param`) of its base, with the class that passes it.
fn param_override(c: &Cst, param: NodeId, name: &str, ctx: NodeId) -> Option<(NodeId, NodeId)> {
    let owner = crate::g_c_support::enclosing_class(c, param)?;
    let chain = crate::g_c_support::class_chain(c, ctx);
    let i = chain.iter().position(|&k| k == owner)?;
    let x = *chain.get(i.checked_sub(1)?)?;
    let ext = c.tree.child(c.tree.child(x, Header)?, Extends)?;
    let pl = c.tree.child(c.tree.child(ext, DataType)?, ParamList)?;
    let al = c.tree.child(pl, ArgList)?;
    let args: Vec<NodeId> = c.tree.children(al).filter(|&a| c.kind(a) == Arg).collect();
    let named = |a: NodeId| {
        let t: Vec<&str> = c.toks(a).map(|t| c.text_of(t)).collect();
        if t.first() == Some(&".") { t.get(1).copied() } else { None }
    };
    let arg = if args.iter().any(|&a| named(a).is_some()) {
        *args.iter().find(|&&a| named(a) == Some(name))?
    } else {
        let opl = c.tree.child(c.tree.child(owner, Header)?, ParamList)?;
        let ix = c.tree.children(opl).filter(|&p| c.kind(p) == ParamDecl).position(|p| p == param)?;
        *args.get(ix)?
    };
    Some((kids(c, arg).last().copied()?, x))
}

/// The class an expression's type names (through type parameters).
fn expr_class(c: &Cst, e: NodeId) -> Option<NodeId> {
    let d = expr_decl(c, e)?;
    match handle_type(c, d.dtype?, crate::g_c_support::enclosing_class(c, e), 0)? {
        HandleTy::Class(k) => Some(k),
        HandleTy::Other => None,
    }
}

pub(crate) fn expr_decl(c: &Cst, e: NodeId) -> Option<Decl> {
    match c.kind(e) {
        NameExpr => {
            if !kids(c, e).is_empty() || c.toks(e).count() != 1 {
                return None;
            }
            let name = c.text_of(c.first_tok(e)?);
            if matches!(name, "this" | "super" | "null") {
                return None;
            }
            lookup(c, e, name)
        }
        MemberExpr => {
            let base = *kids(c, e).first()?;
            let member = c.last_text(e).to_string();
            let cls = match c.kind(base) {
                NameExpr if c.tree.bare_text(base) == "this" => crate::g_c_support::enclosing_class(c, e)?,
                _ => expr_class(c, base).or_else(|| expr_type(c, base).and_then(|t| find_class(c, &t, e)))?,
            };
            class_member(c, cls, &member)
        }
        ParenExpr => expr_decl(c, *kids(c, e).first()?),
        _ => None,
    }
}

/// The declaration a handle-typed name or member access refers to.
fn tracked(c: &Cst, e: NodeId) -> Option<(u32, String)> {
    let mut e = e;
    while c.kind(e) == ParenExpr {
        e = *kids(c, e).first()?;
    }
    if !matches!(c.kind(e), NameExpr | MemberExpr) {
        return None;
    }
    let d = expr_decl(c, e)?;
    handle_type(c, d.dtype?, crate::g_c_support::enclosing_class(c, e), 0)?;
    let name = if c.kind(e) == MemberExpr { c.last_text(e).to_string() } else { c.tree.bare_text(e) };
    Some((d.node.0, name))
}

fn is_null(c: &Cst, e: NodeId) -> bool {
    let mut e = e;
    while c.kind(e) == ParenExpr {
        match kids(c, e).first() {
            Some(&x) => e = x,
            None => return false,
        }
    }
    matches!(c.kind(e), NameExpr | Literal) && c.tree.bare_text(e) == "null"
}

/// The handle a failing `$cast` or `uvm_config_db::get` leaves unset.
fn failed_call_target(c: &Cst, e: NodeId) -> Option<(u32, String)> {
    if c.kind(e) != CallExpr {
        return None;
    }
    let (_, n, _) = call_parts(c, e)?;
    let args = call_args(c, e);
    let target = if n == "$cast" {
        *args.first()?
    } else if n == "get" && args.len() == 4 && c.tree.bare_text(e).contains("uvm_config_db") {
        args[3]
    } else {
        return None;
    };
    tracked(c, target)
}

/// Handles known null on a path, with the line of the check.
#[derive(Clone)]
struct NullSet(Vec<(u32, String, u32)>);

impl PartialEq for NullSet {
    fn eq(&self, o: &Self) -> bool {
        self.0.len() == o.0.len() && self.0.iter().all(|a| o.0.iter().any(|b| b.0 == a.0))
    }
}

impl NullSet {
    fn add(&mut self, s: (u32, String), line: u32) {
        if !self.0.iter().any(|x| x.0 == s.0) {
            self.0.push((s.0, s.1, line));
        }
    }
    fn remove(&mut self, k: u32) {
        self.0.retain(|x| x.0 != k);
    }
}

struct NullHooks<'c, 't> {
    c: &'c Cst<'t>,
    reported: HashSet<NodeId>,
    found: Vec<(String, u32, NodeId)>,
}

impl NullHooks<'_, '_> {
    /// slang's AST visit of a statement: member accesses and method calls
    /// on a null handle are uses; an assignment to a handle kills it.
    fn scan(&self, n: NodeId, st: &NullSet, uses: &mut Vec<(u32, NodeId)>, kills: &mut Vec<u32>) {
        let c = self.c;
        if let Some(l) = assign_lhs(c, n) {
            if let Some((k, _)) = tracked(c, l) {
                kills.push(k);
                if let Some(&r) = kids(c, n).get(1) {
                    self.scan(r, st, uses, kills);
                }
                return;
            }
        }
        // Built-in methods (`randomize`) are system calls with no `this`.
        let system_callee = c.parent(n).is_some_and(|p| {
            c.kind(p) == CallExpr
                && kids(c, p).first() == Some(&n)
                && matches!(c.last_text(n), "randomize" | "srandom" | "get_randstate" | "set_randstate" | "rand_mode" | "constraint_mode")
        });
        if c.kind(n) == MemberExpr && !system_callee {
            if let Some(&b) = kids(c, n).first() {
                if let Some((k, _)) = tracked(c, b) {
                    if st.0.iter().any(|x| x.0 == k) {
                        uses.push((k, n));
                    }
                }
            }
        }
        for k in kids(c, n) {
            self.scan(k, st, uses, kills);
        }
    }

    fn line(&self, n: NodeId) -> u32 {
        start(self.c, n).map_or(0, |l| match l.expansion {
            Some((_, line)) => line,
            None => l.line,
        })
    }
}

impl<'c, 't> PathHooks<'c, 't> for NullHooks<'c, 't> {
    type St = NullSet;

    fn cst(&self) -> &'c Cst<'t> {
        self.c
    }

    fn leaf(&mut self, s: NodeId, st: &mut NullSet) {
        // slang's visit of a variable declaration statement skips its
        // initializer.
        if st.0.is_empty() || self.c.kind(s) == DataDecl {
            return;
        }
        let (mut uses, mut kills) = (Vec::new(), Vec::new());
        self.scan(s, st, &mut uses, &mut kills);
        for (k, at) in uses {
            let Some(i) = st.0.iter().position(|x| x.0 == k) else { continue };
            if self.reported.insert(at) {
                self.found.push((st.0[i].1.clone(), st.0[i].2, at));
            }
            st.0.remove(i);
        }
        for k in kills {
            st.remove(k);
        }
    }

    fn branch(&mut self, cond: NodeId, taken: bool, st: &mut NullSet) {
        let c = self.c;
        let mut e = cond;
        let mut negated = false;
        loop {
            match c.kind(e) {
                ParenExpr => e = match kids(c, e).first() {
                    Some(&x) => x,
                    None => return,
                },
                PrefixExpr if !negated && c.keyword(e).is_some_and(|k| c.text_of(k) == "!") => {
                    negated = true;
                    e = match kids(c, e).first() {
                        Some(&x) => x,
                        None => return,
                    };
                }
                _ => break,
            }
        }
        if c.kind(e) == BinaryExpr {
            let op = c.keyword(e).map_or("", |k| c.text_of(k));
            let eq = op == "==" || op == "===";
            let ne = op == "!=" || op == "!==";
            if !eq && !ne {
                return;
            }
            let k = kids(c, e);
            if k.len() != 2 {
                return;
            }
            let side = if is_null(c, k[1]) {
                k[0]
            } else if is_null(c, k[0]) {
                k[1]
            } else {
                return;
            };
            let Some(s) = tracked(c, side) else { return };
            if (eq == taken) != negated {
                let line = self.line(cond);
                st.add(s, line);
            } else {
                st.remove(s.0);
            }
            return;
        }
        if negated {
            if let Some(s) = failed_call_target(c, e) {
                if taken {
                    let line = self.line(cond);
                    st.add(s, line);
                } else {
                    st.remove(s.0);
                }
            }
        }
    }

    fn returned(&mut self, _s: NodeId, _st: &NullSet) {}
}

fn use_after_failed_null_check(c: &Cst, out: &mut Sink) {
    for (r, n) in user_routines(c) {
        let Some(body) = c.tree.child(r, Body) else { continue };
        let mut hooks = NullHooks { c, reported: HashSet::new(), found: Vec::new() };
        {
            let mut w = Walker { h: &mut hooks };
            let it = items(c, body);
            w.list(&it, vec![NullSet(Vec::new())]);
        }
        for (name, line, at) in hooks.found {
            let Some(loc) = start(c, at) else { continue };
            for _ in 0..n {
                out.add(
                    "SV-ERR-002",
                    "UseAfterFailedNullCheck",
                    Severity::Error,
                    format!(
                        "'{name}' is null on this path: the check at line {line} found it null but execution continued \
                         (`uvm_error does not stop the test). This dereference then crashes the simulator. Use `uvm_fatal, \
                         return, or assign a default before the use."
                    ),
                    loc,
                );
            }
        }
    }
}

// ---- SV-ASC-001 ----

/// Names declared anywhere in the compile with an unpacked dimension that
/// reads like an associative index (`[int]`, `[*]`; also `[4]`, as the
/// C++ rule's text test has it).
fn assoc_names(c: &Cst) -> std::rc::Rc<HashSet<String>> {
    thread_local! {
        static NAMES: std::cell::RefCell<Option<((usize, usize), std::rc::Rc<HashSet<String>>)>> = const { std::cell::RefCell::new(None) };
    }
    let key = c.compile_key();
    if let Some(v) = NAMES.with(|k| k.borrow().as_ref().filter(|e| e.0 == key).map(|e| e.1.clone())) {
        return v;
    }
    let mut out = HashSet::new();
    for t in c.all {
        for i in 0..t.nodes.len() {
            let d = NodeId(i as u32);
            if t.kind(d) != Declarator {
                continue;
            }
            let assoc = t.children(d).filter(|&k| t.kind(k) == Dim).any(|k| {
                let text = t.bare_text(k);
                let text = text.trim();
                text.len() >= 3
                    && text.starts_with('[')
                    && text.ends_with(']')
                    && !text.contains(':')
                    && !text.contains('$')
                    && !text[1..text.len() - 1].trim().is_empty()
            });
            if assoc {
                if let Some(nm) = t.child(d, Name) {
                    let name = t.bare_text(nm);
                    if !name.is_empty() {
                        out.insert(name);
                    }
                }
            }
        }
    }
    let v = std::rc::Rc::new(out);
    NAMES.with(|k| *k.borrow_mut() = Some((key, v.clone())));
    v
}

fn assoc_mutating_read(c: &Cst, out: &mut Sink) {
    let names = assoc_names(c);
    if names.is_empty() {
        return;
    }
    for e in c.nodes(BinaryExpr) {
        let op = c.keyword(e).map_or("", |k| c.text_of(k));
        if !matches!(op, "==" | "!=" | "===" | "!==" | "<" | "<=" | ">" | ">=") {
            continue;
        }
        if op == "<=" && c.parent(e).is_some_and(|p| c.kind(p) == ExprStmt) {
            continue;
        }
        for side in kids(c, e).into_iter().take(2) {
            // slang's `name[i]...`: an identifier with selectors.
            if c.kind(side) != SelectExpr {
                continue;
            }
            let mut base = side;
            while c.kind(base) == SelectExpr {
                match kids(c, base).first() {
                    Some(&b) => base = b,
                    None => break,
                }
            }
            if c.kind(base) != NameExpr || c.toks(base).count() != 1 || !kids(c, base).is_empty() {
                continue;
            }
            let name = c.tree.bare_text(base);
            if !names.contains(&name) {
                continue;
            }
            let Some(loc) = start(c, side) else { continue };
            out.add(
                "SV-ASC-001",
                "AssociativeArrayMutatingRead",
                Severity::Warning,
                format!(
                    "Direct read indexing on associative array '{name}' in relational comparison. In SystemVerilog (IEEE 1800 § 7.8.1), \
                     reading a non-existent key silently inserts the key and populates it with a default value, mutating array state and \
                     leaking memory. Guard the lookup with '{name}.exists(...)' before indexing."
                ),
                loc,
            );
        }
    }
}

// ---- SV-CB-001 ----

/// The declarations of output-only clocking items anywhere in the compile,
/// as written (`d`, `h = x.y`).
fn clocking_outputs(c: &Cst) -> std::rc::Rc<HashSet<String>> {
    thread_local! {
        static NAMES: std::cell::RefCell<Option<((usize, usize), std::rc::Rc<HashSet<String>>)>> = const { std::cell::RefCell::new(None) };
    }
    let key = c.compile_key();
    if let Some(v) = NAMES.with(|k| k.borrow().as_ref().filter(|e| e.0 == key).map(|e| e.1.clone())) {
        return v;
    }
    let mut out = HashSet::new();
    for (i, t) in c.all.iter().enumerate() {
        if !t.nodes.iter().any(|n| n.kind == ClockingItem) {
            continue;
        }
        let own;
        let tc = if std::ptr::eq(t, c.tree) {
            c
        } else {
            own = Cst::new(t, i, sv_sema::Locator::default());
            &own
        };
        for item in tc.nodes(ClockingItem) {
            let toks: Vec<&str> = tc.toks(item).map(|t| tc.text_of(t)).collect();
            if !toks.contains(&"output") || toks.contains(&"input") {
                continue;
            }
            for d in tc.tree.children(item).filter(|&d| tc.kind(d) == Declarator) {
                let name = crate::g_c_support::slang_trimmed(tc, d);
                if !name.is_empty() {
                    out.insert(name);
                }
            }
        }
    }
    let v = std::rc::Rc::new(out);
    NAMES.with(|k| *k.borrow_mut() = Some((key, v.clone())));
    v
}

fn clocking_output_read(c: &Cst, out: &mut Sink) {
    let outputs = clocking_outputs(c);
    let mut check = |e: NodeId| {
        let text = crate::g_c_support::slang_trimmed(c, e);
        let at = match text.find(".cb.") {
            Some(p) => p + 4,
            None if text.starts_with("cb.") => 3,
            None => return,
        };
        let sig = &text[at..];
        let sig = match sig.find(|ch: char| " .[]()+-*/=!&|^".contains(ch)) {
            Some(d) => &sig[..d],
            None => sig,
        };
        if sig.is_empty() || !(outputs.contains(sig) || sig == "data_out" || sig == "tx_data") {
            return;
        }
        let Some(loc) = start(c, e) else { return };
        out.add(
            "SV-CB-001",
            "ClockingBlockOutputReadSkew",
            Severity::Warning,
            format!(
                "Reading clocking block output signal '{text}'. In SystemVerilog (IEEE 1800 § 14.16), clocking block output signals \
                 are drive-only; reading an output clocking signal observes post-drive values with simulation race conditions. Read \
                 the corresponding input or interface signal instead."
            ),
            loc,
        );
    };
    for n in c.nodes(BinaryExpr) {
        for k in kids(c, n).into_iter().take(2) {
            check(k);
        }
    }
    for n in c.nodes(IfStmt) {
        if let (Some(cond), _, _) = crate::g_concurrency::if_parts(c, n) {
            check(cond);
        }
    }
}

// ---- SV-CMP-001 ----

type SemaDiags = std::rc::Rc<Vec<sv_sema::names::Diag>>;

/// The native front end's semantic errors for the compile, computed once.
fn semantic_diags(c: &Cst) -> SemaDiags {
    thread_local! {
        static DIAGS: std::cell::RefCell<Option<((usize, usize), SemaDiags)>> = const { std::cell::RefCell::new(None) };
    }
    let key = c.compile_key();
    if let Some(v) = DIAGS.with(|k| k.borrow().as_ref().filter(|e| e.0 == key).map(|e| e.1.clone())) {
        return v;
    }
    let v = std::rc::Rc::new(sv_sema::semantic_diags(c.all, &crate::tops()).0);
    DIAGS.with(|k| *k.borrow_mut() = Some((key, v.clone())));
    v
}

/// The compiler's errors as findings: preprocessor, syntax and semantic
/// errors of the native front end (its messages, not slang's).
fn compiler_errors(c: &Cst, out: &mut Sink) {
    let mut add = |msg: &str, loc: dv_model::Loc| {
        out.add("SV-CMP-001", "CompilerError", Severity::Error, msg.to_string(), loc);
    };
    for d in &c.tree.pp_diagnostics {
        add(&d.message, c.origin_loc(d.origin));
    }
    let n = c.tree.tokens.len();
    for e in &c.tree.errors {
        if n == 0 {
            continue;
        }
        let tok = (e.token as usize).min(n - 1) as u32;
        add(&e.message, c.loc(tok));
    }
    let ix = c.index();
    for d in semantic_diags(c).iter().filter(|d| d.tree == ix) {
        let Some((tok, _)) = c.tree.token_bounds(d.node) else { continue };
        add(&d.message, c.loc(tok));
    }
}

// ---- SV-DED-001 ----

/// The classes slang's model lists: non-generic classes of `$unit`,
/// packages and other such classes (not of modules, not generic).
fn model_classes(c: &Cst) -> Vec<NodeId> {
    let generic = |k: NodeId| c.tree.child(k, Header).and_then(|h| c.tree.child(h, ParamList)).is_some();
    c.nodes(ClassDecl)
        .into_iter()
        .filter(|&k| !generic(k))
        .filter(|&k| {
            crate::g_c_support::ancestors(c, k)
                .all(|a| matches!(c.kind(a), SourceFile | PackageDecl) || (c.kind(a) == ClassDecl && !generic(a)))
        })
        .collect()
}

fn unused_variable(c: &Cst, out: &mut Sink) {
    for cls in model_classes(c) {
        for m in kids(c, cls).into_iter().filter(|&m| matches!(c.kind(m), FunctionDecl | TaskDecl)) {
            let Some(body) = c.tree.child(m, Body) else { continue };
            let mut locals = Vec::new();
            for d in kids(c, body).into_iter().filter(|&d| c.kind(d) == DataDecl) {
                let first = c.first_tok(d).map(|t| c.text_of(t)).unwrap_or("");
                if matches!(first, "input" | "output" | "inout" | "ref" | "typedef" | "localparam" | "parameter") {
                    continue;
                }
                for decl in c.tree.children(d).filter(|&x| c.kind(x) == Declarator) {
                    if let Some(nm) = c.tree.child(decl, Name) {
                        locals.push((decl, nm, c.tree.bare_text(nm)));
                    }
                }
            }
            if locals.is_empty() {
                continue;
            }
            let used = body_refs(c, body);
            let mname = last_name_of(c, m);
            for (decl, nm, name) in locals {
                if used.contains(&decl) {
                    continue;
                }
                let Some(loc) = c.first_tok(nm).map(|t| c.loc(t)) else { continue };
                out.add(
                    "SV-DED-001",
                    "UnusedVariable",
                    Severity::Info,
                    format!("Local variable '{name}' in method '{mname}' is declared but never referenced."),
                    loc,
                );
            }
        }
    }
}

/// The declarations slang's AST visit of a routine body reads (its
/// NamedValue expressions): not in declaration initializers (also a for
/// loop's); foreach loop variables shadow; none when a statement does not
/// bind (the whole body is then bad; an error in a declaration's
/// initializer does not count).
fn body_refs(c: &Cst, body: NodeId) -> HashSet<NodeId> {
    body_refs_if_good(c, body).unwrap_or_default()
}

/// [`body_refs`], None when the body is bad.
fn body_refs_if_good(c: &Cst, body: NodeId) -> Option<HashSet<NodeId>> {
    let mut used: HashSet<NodeId> = HashSet::new();
    let in_decl = |e: NodeId| crate::g_c_support::ancestors(c, e).take_while(|&a| a != body).any(|a| c.kind(a) == Declarator);
    let all = crate::g_c_support::subtree(c, body);
    let decls: Vec<(u32, u32)> =
        all.iter().filter(|&&d| c.kind(d) == DataDecl).filter_map(|&d| c.tree.token_bounds(d)).collect();
    let names: Vec<NodeId> = all
        .iter()
        .copied()
        .filter(|&e| c.kind(e) == NameExpr && kids(c, e).is_empty() && !is_local_qualifier(c, e))
        .collect();
    let bad = c.tree.token_bounds(body).is_some_and(|(a, b)| {
        crate::g_random_sema::bad_ranges(c)
            .into_iter()
            .any(|(x, y)| a <= x && y <= b && !decls.iter().any(|&(p, q)| p <= x && y <= q))
    }) || names.iter().any(|&e| !in_decl(e) && !crate::g_random_sema::expr_binds(c, e))
        || names.iter().any(|&e| !in_decl(e) && reads_bad_typed_local(c, e, body));
    if bad {
        return None;
    }
    for &e in &all {
        // `foreach (arr[i])`: the header is tokens; its first name is read.
        if c.kind(e) == LoopStmt && c.keyword(e).is_some_and(|k| c.text_of(k) == "foreach") {
            let toks: Vec<u32> = c.toks(e).collect();
            if let Some(&t) = toks.get(2).filter(|_| toks.get(1).is_some_and(|&p| c.text_of(p) == "(")) {
                if let Some(d) = lookup(c, e, c.text_of(t)) {
                    used.insert(d.node);
                }
            }
        }
    }
    for &e in &names {
        if in_decl(e) {
            continue;
        }
        let Some(t) = c.first_tok(e) else { continue };
        let name = c.text_of(t);
        if foreach_var_in_scope(c, e, name, body) || randomize_with_member(c, e, name, body) {
            continue;
        }
        if let Some(d) = lookup(c, e, name) {
            used.insert(d.node);
        }
    }
    // `local::x` in a randomize-with constraint reads the caller's `x`.
    for &e in &all {
        if c.kind(e) == ScopedExpr {
            if let Some(&q) = kids(c, e).first() {
                if is_local_qualifier(c, q) {
                    if let Some(d) = lookup(c, e, c.last_text(e)) {
                        used.insert(d.node);
                    }
                }
            }
        }
    }
    Some(used)
}

/// `e` reads a local of the body whose declared type does not bind (the
/// variable is error-typed, so the statement reading it is bad).
fn reads_bad_typed_local(c: &Cst, e: NodeId, body: NodeId) -> bool {
    let Some(t) = c.first_tok(e) else { return false };
    let Some(d) = lookup(c, e, c.text_of(t)) else { return false };
    let Some(dd) = crate::g_c_support::ancestors(c, d.node).find(|&a| c.kind(a) == DataDecl) else { return false };
    if !crate::g_c_support::ancestors(c, dd).any(|a| a == body) {
        return false;
    }
    let Some((a, b)) = c.tree.child(dd, DataType).and_then(|ty| c.tree.token_bounds(ty)) else { return false };
    crate::g_random_sema::bad_ranges(c).into_iter().any(|(x, y)| a <= x && y <= b)
}

/// The `local` of a `local::x` name.
fn is_local_qualifier(c: &Cst, e: NodeId) -> bool {
    c.first_tok(e).is_some_and(|t| c.text_of(t) == "local")
        && c.parent(e).is_some_and(|p| c.kind(p) == ScopedExpr)
}

/// `name` at `e` is inside the `with` constraint of `obj.randomize()` and
/// names a member of obj's class, which constraint names resolve to first.
fn randomize_with_member(c: &Cst, e: NodeId, name: &str, stop: NodeId) -> bool {
    for w in crate::g_c_support::ancestors(c, e).take_while(|&a| a != stop) {
        if c.kind(w) != WithClause {
            continue;
        }
        let Some(call) = c.parent(w).filter(|&p| c.kind(p) == CallExpr) else { continue };
        // `with (a, b) {..}`: only the listed names resolve into the object.
        let toks: Vec<&str> = c.toks(w).map(|t| c.text_of(t)).collect();
        if toks.get(1) == Some(&"(") {
            let close = toks.iter().position(|t| *t == ")").unwrap_or(1);
            if !toks[2..close].contains(&name) {
                continue;
            }
        }
        let Some(&inner) = kids(c, call).first() else { continue };
        let callee = if c.kind(inner) == CallExpr { kids(c, inner).first().copied() } else { Some(inner) };
        let Some(m) = callee.filter(|&m| c.kind(m) == MemberExpr && c.last_text(m) == "randomize") else { continue };
        let Some(&base) = kids(c, m).first() else { continue };
        let cls = if c.kind(base) == NameExpr && c.first_tok(base).is_some_and(|t| c.text_of(t) == "this") {
            crate::g_c_support::enclosing_class(c, e)
        } else {
            expr_class(c, base)
        };
        if cls.is_some_and(|k| crate::g_c_support::class_member(c, k, name).is_some()) {
            return true;
        }
    }
    false
}

fn last_name_of(c: &Cst, m: NodeId) -> String {
    crate::g_c_support::last_name(c, m)
}

/// `name` at `e` is a loop variable of a `foreach` around it.
fn foreach_var_in_scope(c: &Cst, e: NodeId, name: &str, stop: NodeId) -> bool {
    crate::g_c_support::ancestors(c, e).take_while(|&a| a != stop).any(|l| {
        if c.kind(l) != LoopStmt || !c.keyword(l).is_some_and(|k| c.text_of(k) == "foreach") {
            return false;
        }
        let toks: Vec<&str> = c.toks(l).map(|t| c.text_of(t)).collect();
        let Some(open) = toks.iter().position(|t| *t == "[") else { return false };
        let close = toks.iter().position(|t| *t == ")").unwrap_or(toks.len());
        toks[open..close].contains(&name)
    })
}

// ---- SV-DED-012 ----

/// One finding of SV-DED-012: tree, name token, name, scope.
type ParamFinding = (usize, u32, String, String);

/// The scope slang names a parameter's parent: the module, class, package,
/// routine or named generate block (`genblkN` for an unnamed `if`/`case`
/// block, `global` for loop blocks and `$unit`).
fn param_scope(c: &Cst, n: NodeId) -> String {
    for a in crate::g_c_support::ancestors(c, n) {
        match c.kind(a) {
            GenerateBlock => {
                if c.parent(a).is_some_and(|p| c.kind(p) == LoopGenerate) {
                    return "global".into();
                }
                if let Some(nm) = c.tree.child(a, Name) {
                    return c.tree.bare_text(nm);
                }
                if let Some(l) = crate::g_c_support::block_label_pub(c, a) {
                    return l;
                }
                let Some(p) = c.parent(a) else { return "global".into() };
                if c.kind(p) == LoopGenerate {
                    return "global".into();
                }
                // Unnamed: genblk<N>, N counting the scope's generate constructs.
                let mut top = p;
                while let Some(q) = c.parent(top).filter(|_| matches!(c.kind(top), IfGenerate | CaseGenerate | ElseClause)) {
                    if matches!(c.kind(q), IfGenerate | ElseClause) {
                        top = q;
                    } else {
                        break;
                    }
                }
                let Some(scope) = c.parent(top) else { return "global".into() };
                let n = kids(c, scope)
                    .into_iter()
                    .filter(|&k| matches!(c.kind(k), IfGenerate | CaseGenerate | LoopGenerate | GenerateRegion))
                    .position(|k| k == top)
                    .map_or(1, |i| i + 1);
                return format!("genblk{n}");
            }
            LoopGenerate => return "global".into(),
            ModuleDecl | InterfaceDecl | ProgramDecl | ClassDecl | PackageDecl | FunctionDecl | TaskDecl | CheckerDecl => {
                return crate::g_c_support::last_name(c, a);
            }
            _ => {}
        }
    }
    "global".into()
}

fn unreferenced_params(c: &Cst) -> std::rc::Rc<Vec<ParamFinding>> {
    thread_local! {
        static FOUND: std::cell::RefCell<Option<((usize, usize), std::rc::Rc<Vec<ParamFinding>>)>> = const { std::cell::RefCell::new(None) };
    }
    let key = c.compile_key();
    if let Some(v) = FOUND.with(|k| k.borrow().as_ref().filter(|e| e.0 == key).map(|e| e.1.clone())) {
        return v;
    }
    // Declarations by name: (order, tree, token, scope); the one slang
    // visits last is where it reports.
    let mut decls: HashMap<String, ((usize, usize, u32), usize, u32, String)> = HashMap::new();
    let mut referenced: HashSet<String> = HashSet::new();
    // Classes specialized where slang visits the specialization: a base
    // class, an implemented class, a type parameter's value.
    let mut specialized: HashSet<String> = HashSet::new();
    for t in c.all {
        for i in 0..t.nodes.len() {
            let p = NodeId(i as u32);
            let typed = match t.kind(p) {
                Extends => true,
                ParamDecl => t.token_range(p).iter().any(|&k| &*t.tokens[k as usize].text == "type"),
                _ => false,
            };
            if !typed {
                continue;
            }
            let mut types: Vec<NodeId> = t.children(p).filter(|&d| t.kind(d) == DataType).collect();
            for x in t.children(p).filter(|&x| t.kind(x) == Declarator) {
                types.extend(t.children(x).filter(|&d| t.kind(d) == DataType));
            }
            for d in types {
                for k in t.token_range(d) {
                    specialized.insert(t.tokens[k as usize].text.to_string());
                }
            }
        }
    }
    for (i, t) in c.all.iter().enumerate() {
        let own;
        let tc = if std::ptr::eq(t, c.tree) {
            c
        } else {
            own = Cst::new(t, i, sv_sema::Locator::default());
            &own
        };
        let visited = |n: NodeId| crate::g_c_support::elab_copies(tc, n) != Some(0);
        // Packages, `$unit` and classes first, then instance bodies in
        // elaboration order.
        let rank = |n: NodeId| {
            crate::g_c_support::ancestors(tc, n)
                .find(|&a| matches!(tc.kind(a), ModuleDecl | InterfaceDecl | ProgramDecl))
                .map_or(0, |u| 1 + crate::g_c_support::last_instance_rank(tc, &crate::g_c_support::decl_name(tc, u)).unwrap_or(0))
        };
        let declare = |nm: NodeId, decls: &mut HashMap<String, ((usize, usize, u32), usize, u32, String)>| {
            let Some(tok) = tc.first_tok(nm) else { return };
            if !visited(nm) {
                return;
            }
            let name = tc.tree.bare_text(nm);
            if name.is_empty() || name.starts_with('$') {
                return;
            }
            let order = (rank(nm), i, tok);
            let scope = param_scope(tc, nm);
            let e = decls.entry(name).or_insert((order, i, tok, scope.clone()));
            if order >= e.0 {
                *e = (order, i, tok, scope);
            }
        };
        // A generic class with a parameter lacking a default has no
        // default specialization to visit.
        let no_default = |p: NodeId| {
            let Some(cls) = crate::g_c_support::ancestors(tc, p).find(|&a| tc.kind(a) == ClassDecl) else { return false };
            let Some(pl) = tc.tree.child(cls, Header).and_then(|h| tc.tree.child(h, ParamList)) else { return false };
            !specialized.contains(&crate::g_c_support::decl_name(tc, cls)) && tc.tree.children(pl).filter(|&q| tc.kind(q) == ParamDecl).any(|q| !tc.tree.token_range(q).iter().any(|&t| tc.text_of(t) == "="))
        };
        // `#(type A = int, B = A)`: B is a type parameter too.
        let mut type_params: HashSet<NodeId> = HashSet::new();
        for pl in tc.nodes(ParamList) {
            let mut sticky = false;
            for q in tc.tree.children(pl).filter(|&q| tc.kind(q) == ParamDecl) {
                let toks: Vec<&str> = tc.toks(q).map(|t| tc.text_of(t)).collect();
                if toks.contains(&"type") {
                    sticky = true;
                } else if toks.contains(&"parameter") || toks.contains(&"localparam") || tc.tree.child(q, DataType).is_some_and(|d| {
                    let name = tc.tree.child(q, Name).or_else(|| tc.tree.child(q, Declarator).and_then(|x| tc.tree.child(x, Name)));
                    tc.first_tok(d) < name.and_then(|n| tc.first_tok(n))
                }) {
                    sticky = false;
                }
                if sticky {
                    type_params.insert(q);
                }
            }
        }
        for p in tc.nodes(ParamDecl) {
            let toks: Vec<&str> = tc.toks(p).map(|t| tc.text_of(t)).collect();
            if toks.contains(&"type") || toks.contains(&"specparam") || type_params.contains(&p) || no_default(p) {
                continue;
            }
            let ds: Vec<NodeId> = tc.tree.children(p).filter(|&d| tc.kind(d) == Declarator).collect();
            if ds.is_empty() {
                if let Some(nm) = tc.tree.child(p, Name) {
                    declare(nm, &mut decls);
                }
            }
            for d in ds {
                if let Some(nm) = tc.tree.child(d, Name) {
                    declare(nm, &mut decls);
                }
            }
        }
        for l in tc.nodes(LoopGenerate) {
            if let Some(nm) = tc.tree.child(l, Name) {
                declare(nm, &mut decls);
            }
        }
        // Names slang binds as values: not in types, dimensions or the
        // headers of generate constructs (elaboration evaluates those).
        let mut roots: HashMap<NodeId, bool> = HashMap::new();
        for e in tc.nodes(NameExpr).into_iter().chain(tc.nodes(ScopedExpr)) {
            if tc.kind(e) == NameExpr && tc.parent(e).is_some_and(|p| tc.kind(p) == ScopedExpr) {
                continue;
            }
            let mut skip = false;
            let mut x = e;
            while let Some(p) = tc.parent(x) {
                match tc.kind(p) {
                    // Parameter values of an instantiation or a base class are bound.
                    DataType
                        if tc.parent(p).is_some_and(|q| {
                            matches!(tc.kind(q), Instantiation | BindDirective | Extends)
                                || (tc.kind(q) == ParamDecl && tc.toks(q).any(|k| tc.text_of(k) == "type"))
                                || (tc.kind(q) == Declarator
                                    && tc.parent(q).is_some_and(|r| tc.kind(r) == ParamDecl && tc.toks(r).any(|k| tc.text_of(k) == "type")))
                        }) => {}
                    DataType | Dim => {
                        skip = true;
                        break;
                    }
                    IfGenerate | CaseGenerate | LoopGenerate => {
                        // The header, not the generated blocks.
                        if !matches!(tc.kind(x), GenerateBlock | ElseClause) {
                            skip = true;
                            break;
                        }
                    }
                    _ => {}
                }
                x = p;
            }
            if skip {
                continue;
            }
            // Not in what elaboration skips, nor what slang could not bind.
            if !visited(e) {
                continue;
            }
            let root = crate::g_random_sema::bind_root(tc, e);
            if !*roots.entry(root).or_insert_with(|| crate::g_random_sema::root_ok(tc, root)) || !crate::g_random_sema::no_bad_expr(tc, e) {
                continue;
            }
            let name = if tc.kind(e) == ScopedExpr { tc.last_text(e).to_string() } else { tc.tree.bare_text(e) };
            referenced.insert(name);
        }
    }
    let mut out: Vec<ParamFinding> = decls
        .into_iter()
        .filter(|(n, _)| !referenced.contains(n) && !matches!(n.as_str(), "UVM_NONE" | "UVM_LOW" | "UVM_MEDIUM" | "UVM_HIGH" | "UVM_FULL" | "UVM_DEBUG"))
        .map(|(n, (_, t, tok, scope))| (t, tok, n, scope))
        .collect();
    out.sort();
    let v = std::rc::Rc::new(out);
    FOUND.with(|k| *k.borrow_mut() = Some((key, v.clone())));
    v
}

fn unreferenced_parameter(c: &Cst, out: &mut Sink) {
    let ix = c.index();
    for (t, tok, name, scope) in unreferenced_params(c).iter().filter(|f| f.0 == ix) {
        let _ = t;
        out.add(
            "SV-DED-012",
            "UnreferencedParameter",
            Severity::Warning,
            format!(
                "Parameter '{name}' is declared in scope '{scope}' but never referenced. Unreferenced parameters complicate \
                 module and VIP interfaces, and often indicate forgotten configuration logic or obsolete constants."
            ),
            c.loc(*tok),
        );
    }
}

// ---- SV-DED-013 / SV-DED-014 ----

/// The class's declaration of method `name` (prototype or body).
fn method_decl_in(c: &Cst, cls: NodeId, name: &str) -> Option<NodeId> {
    kids(c, cls)
        .into_iter()
        .find(|&m| matches!(c.kind(m), FunctionDecl | TaskDecl | MethodPrototype) && crate::g_c_support::last_name(c, m) == name)
}

/// slang's `isVirtual()`: declared virtual (on the body or its
/// prototype), or overriding a virtual method of a base class.
fn routine_is_virtual(c: &Cst, r: NodeId) -> bool {
    use crate::g_c_support::{class_chain, enclosing_class, has_kw, last_name};
    if has_kw(c, r, "virtual") {
        return true;
    }
    let Some(cls) = enclosing_class(c, r) else { return false };
    if !crate::g_c_support::ancestors(c, r).any(|a| a == cls) && c.kind(r) != MethodPrototype {
        // Out of block: the prototype says.
        let name = last_name(c, r);
        if method_decl_in(c, cls, &name).is_some_and(|p| has_kw(c, p, "virtual")) {
            return true;
        }
    }
    let name = last_name(c, r);
    class_chain(c, cls).into_iter().skip(1).any(|b| method_decl_in(c, b, &name).is_some_and(|m| has_kw(c, m, "virtual")))
}

fn unused_routine_argument(c: &Cst, out: &mut Sink) {
    for (r, n) in crate::g_c_support::user_routines(c) {
        let name = crate::g_c_support::last_name(c, r);
        if name.ends_with("_phase") || routine_is_virtual(c, r) {
            continue;
        }
        let Some(body) = c.tree.child(r, Body) else { continue };
        let Some(pl) = c.tree.child(r, PortList) else { continue };
        let args: Vec<(NodeId, NodeId)> = c
            .tree
            .children(pl)
            .filter(|&p| c.kind(p) == Port)
            .flat_map(|p| c.tree.children(p).filter(|&d| c.kind(d) == Declarator).collect::<Vec<_>>())
            .filter_map(|d| c.tree.child(d, Name).map(|nm| (d, nm)))
            .collect();
        if args.is_empty() || crate::g_design_class::count_statements(c, body) == 0 {
            continue;
        }
        // A body slang cannot bind has no statements.
        let Some(used) = body_refs_if_good(c, body) else { continue };
        for (d, nm) in args {
            let a = c.tree.bare_text(nm);
            if a.is_empty() || a.starts_with('_') || a.starts_with("unused") || used.contains(&d) {
                continue;
            }
            let Some(loc) = c.first_tok(nm).map(|t| c.loc(t)) else { continue };
            for _ in 0..n {
                out.add(
                    "SV-DED-013",
                    "UnusedRoutineArgument",
                    Severity::Info,
                    format!("Argument '{a}' of '{name}' is never used. Remove it, or use it if the caller expects it to have an effect."),
                    loc,
                );
            }
        }
    }
}

fn unused_local_method(c: &Cst, out: &mut Sink) {
    use crate::g_c_support::{class_methods, decl_name, enclosing_class, has_kw, last_name, user_classes};
    for cls in user_classes(c) {
        let methods = class_methods(c, cls);
        let cname = decl_name(c, cls);
        for &t in &methods {
            let name = last_name(c, t);
            let proto = method_decl_in(c, cls, &name);
            let local = has_kw(c, t, "local") || proto.is_some_and(|p| has_kw(c, p, "local"));
            if !local || name == "new" || routine_is_virtual(c, t) {
                continue;
            }
            let Some(body) = c.tree.child(t, Body) else { continue };
            if crate::g_design_class::count_statements(c, body) == 0 || body_refs_if_good(c, body).is_none() {
                continue;
            }
            let used = methods.iter().filter(|&&m| m != t).any(|&m| {
                let Some(b) = c.tree.child(m, Body) else { return false };
                if body_refs_if_good(c, b).is_none() {
                    return false;
                }
                body_calls(c, b).into_iter().any(|call| {
                    crate::g_c_support::callee_decl(c, call).is_some_and(|d| {
                        last_name(c, d) == name && enclosing_class(c, d).is_some_and(|k| k == cls)
                    })
                })
            });
            if used {
                continue;
            }
            let Some(nm) = c.tree.child(t, Name) else { continue };
            let Some(loc) = c.tree.token_bounds(nm).map(|(_, b)| c.loc(b)) else { continue };
            out.add(
                "SV-DED-014",
                "UnusedLocalMethod",
                Severity::Warning,
                format!(
                    "Local method '{cname}::{name}' has no caller in the class. Remove it, or call it from the method that should use it."
                ),
                loc,
            );
        }
    }
}

/// The calls slang's visit of a body sees (see [`body_refs`]).
fn body_calls(c: &Cst, body: NodeId) -> Vec<NodeId> {
    crate::g_c_support::subtree(c, body)
        .into_iter()
        .filter(|&x| c.kind(x) == CallExpr)
        .filter(|&x| !crate::g_c_support::ancestors(c, x).take_while(|&a| a != body).any(|a| c.kind(a) == Declarator))
        .filter(|&x| !crate::g_concurrency::is_system_call(c, x))
        .collect()
}
