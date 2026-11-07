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

fn expr_decl(c: &Cst, e: NodeId) -> Option<Decl> {
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
    let key = (c.all.as_ptr() as usize, c.all.len());
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
    let key = (c.all.as_ptr() as usize, c.all.len());
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
