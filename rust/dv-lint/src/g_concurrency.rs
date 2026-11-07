//! Group C of builder slice N6.3: the concurrency rules (forks, waits,
//! locks, threads and loops in time). The C++ rules walk slang's elaborated
//! AST; here they read the syntax tree with [`crate::g_c_support`] for what
//! the AST knows (names, instances). Messages are verbatim.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use dv_model::Loc;
use sv_syntax::{Child, NodeId, SyntaxKind};

use crate::g_c_support::*;
use crate::syntax::Cst;
use crate::Check::Syntax;
use crate::{Rule, Severity, Sink};
use SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-CNC-002", name: "CircularThreadWaitDeadlock", check: Syntax(circular_wait) },
        Rule { id: "SV-CNC-003", name: "PollingLoop", check: Syntax(polling_loop) },
        Rule { id: "SV-CNC-004", name: "SharedWriteFromForks", check: Syntax(shared_write) },
        Rule { id: "SV-CON-006", name: "EventTriggerInAlwaysComb", check: Syntax(trigger_in_comb) },
        Rule { id: "SV-CON-008", name: "MutexTryGetWithoutCheck", check: Syntax(try_get_unchecked) },
        Rule { id: "SV-CON-009", name: "WaitForkWithoutFork", check: Syntax(wait_fork_without_fork) },
        Rule { id: "SV-CON-011", name: "WaitModifiedInLoop", check: Syntax(wait_in_loop) },
        Rule { id: "SV-CON-014", name: "DisableNamedBlockInClass", check: Syntax(disable_in_class) },
        Rule { id: "SV-CON-015", name: "UnguardedBlockingWait", check: Syntax(unguarded_wait) },
        Rule { id: "SV-DLY-003", name: "RepeatLoopWithoutTiming", check: Syntax(repeat_without_timing) },
        Rule { id: "SV-DLY-004", name: "ZeroTimeForeverLoop", check: Syntax(zero_time_forever) },
        Rule { id: "SV-DRV-001", name: "DrivingDuringReset", check: Syntax(driving_during_reset) },
        Rule { id: "SV-EVT-001", name: "ZeroDelayEventWaitLoop", check: Syntax(triggered_wait_loop) },
        Rule { id: "SV-FOR-001", name: "ForkInsideFunction", check: Syntax(fork_in_function) },
        Rule { id: "SV-INT-001", name: "BlockingAssignmentToInterface", check: Syntax(blocking_to_interface) },
        Rule { id: "SV-IPC-001", name: "TryPutIgnoredReturn", check: Syntax(try_put_ignored) },
        Rule { id: "SV-RSC-001", name: "LockNotReleased", check: Syntax(lock_not_released) },
        Rule { id: "SV-RSC-002", name: "FileHandleNotClosed", check: Syntax(file_not_closed) },
        Rule { id: "SV-THR-001", name: "ForkLoopVariableShared", check: Syntax(fork_loop_variable) },
        Rule { id: "SV-THR-002", name: "StaticRoutineVariableLifetime", check: Syntax(static_routine_variable) },
        Rule { id: "SV-THR-003", name: "UnboundedThreadSpawn", check: Syntax(unbounded_spawn) },
    ]
}

/// Adds the same finding `n` times (once per visit of slang's AST walk).
fn add_n(s: &mut Sink, n: usize, rule: &'static str, name: &'static str, sev: Severity, msg: String, loc: Loc) {
    for _ in 0..n {
        s.add(rule, name, sev, msg.clone(), loc);
    }
}

// ---- statements as slang binds them ----

pub(crate) fn is_stmt(k: SyntaxKind) -> bool {
    matches!(
        k,
        SeqBlock
            | ParBlock
            | IfStmt
            | CaseStmt
            | LoopStmt
            | DoWhileStmt
            | TimedStmt
            | WaitStmt
            | AssertStmt
            | RandSequenceStmt
            | EmptyStmt
            | ExprStmt
            | ReturnStmt
            | JumpStmt
            | DisableStmt
            | EventTrigger
            | ProcAssign
            | ConcurrentAssert
            | DataDecl
    )
}

/// The statements (and declarations) of a block or routine body.
pub(crate) fn items(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.tree.children(n).filter(|&k| is_stmt(c.kind(k))).collect()
}

/// A loop's body: the last statement of a `for`/`while`/`repeat`/
/// `forever`/`foreach`, the first of a `do ... while`.
pub(crate) fn loop_body(c: &Cst, l: NodeId) -> Option<NodeId> {
    let k = kids(c, l);
    if c.kind(l) == DoWhileStmt {
        k.into_iter().find(|&x| is_stmt(c.kind(x)))
    } else {
        k.into_iter().filter(|&x| is_stmt(c.kind(x))).last()
    }
}

/// A `while` or `do ... while` condition.
pub(crate) fn loop_cond(c: &Cst, l: NodeId) -> Option<NodeId> {
    let k: Vec<NodeId> = kids(c, l).into_iter().filter(|&x| !matches!(c.kind(x), StmtLabel | Attribute)).collect();
    if c.kind(l) == DoWhileStmt {
        k.get(1).copied()
    } else {
        k.first().copied()
    }
}

pub(crate) fn loop_kw<'a>(c: &'a Cst, l: NodeId) -> &'a str {
    if c.kind(l) == DoWhileStmt {
        return "do";
    }
    c.toks(l).map(|t| c.text_of(t)).find(|t| matches!(*t, "for" | "while" | "repeat" | "forever" | "foreach")).unwrap_or("")
}

/// `forever`, `while` and `do ... while` loops (slang's
/// ForeverLoop/WhileLoop/DoWhileLoop statements).
fn is_wait_loop(c: &Cst, n: NodeId) -> bool {
    c.kind(n) == DoWhileStmt || (c.kind(n) == LoopStmt && matches!(loop_kw(c, n), "forever" | "while"))
}

/// The keyword of a fork's join (`join`, `join_any`, `join_none`).
pub(crate) fn join_kind<'a>(c: &'a Cst, b: NodeId) -> &'a str {
    c.toks(b).map(|t| c.text_of(t)).find(|t| matches!(*t, "join" | "join_any" | "join_none")).unwrap_or("")
}

/// An `if`: its condition, then and else statements.
pub(crate) fn if_parts(c: &Cst, n: NodeId) -> (Option<NodeId>, Option<NodeId>, Option<NodeId>) {
    let k: Vec<NodeId> = kids(c, n).into_iter().filter(|&x| !matches!(c.kind(x), StmtLabel | Attribute)).collect();
    let cond = k.first().copied();
    let then = k.get(1).copied().filter(|&x| c.kind(x) != ElseClause);
    let els = k.iter().copied().find(|&x| c.kind(x) == ElseClause).and_then(|e| kids(c, e).first().copied());
    (cond, then, els)
}

/// The first token of a statement or expression (its label included), where
/// slang's source range starts.
pub(crate) fn start(c: &Cst, n: NodeId) -> Option<Loc> {
    c.first_tok(n).map(|t| c.loc(t))
}

/// The routine's name as slang has it (`f` of `c::f`).
fn routine_name(c: &Cst, r: NodeId) -> String {
    last_name(c, r)
}

/// The kind of token a timing control starts with: `#`, `##` or `@`.
fn timing_kind<'a>(c: &'a Cst, t: NodeId) -> &'a str {
    c.keyword(t).map_or("", |k| c.text_of(k))
}

/// A timed statement's timing control.
fn timing_of(c: &Cst, n: NodeId) -> Option<NodeId> {
    c.tree.child(n, TimingControl)
}

/// `swe::isConstantExpr`: literals, parameters and operators over them.
pub(crate) fn is_constant(c: &Cst, e: NodeId) -> bool {
    match c.kind(e) {
        Literal => true,
        ParenExpr => kids(c, e).first().is_some_and(|&x| is_constant(c, x)),
        NameExpr => {
            let Some(t) = c.first_tok(e) else { return false };
            lookup(c, e, c.text_of(t)).is_some_and(|d| d.kind == DeclKind::Parameter)
        }
        BinaryExpr => {
            let k = kids(c, e);
            k.len() == 2 && is_constant(c, k[0]) && is_constant(c, k[1])
        }
        _ => false,
    }
}

/// A delay control (`#d`) with a constant delay.
fn is_fixed_delay(c: &Cst, t: NodeId) -> bool {
    timing_kind(c, t) == "#" && kids(c, t).first().is_some_and(|&x| is_constant(c, x))
}

/// The calls in a node, in slang's visit order (a call before its
/// arguments).
pub(crate) fn calls_in(c: &Cst, n: NodeId) -> Vec<NodeId> {
    subtree(c, n).into_iter().filter(|&x| c.kind(x) == CallExpr).collect()
}

/// The call is to a system task or function, or a built-in method of an
/// array, queue or string (what slang calls a system call).
pub(crate) fn is_system_call(c: &Cst, call: NodeId) -> bool {
    let Some((callee, name, recv)) = call_parts(c, call) else { return false };
    if name.starts_with('$') {
        return true;
    }
    if c.kind(callee) == ScopedExpr {
        return kids(c, callee).first().and_then(|&x| c.first_tok(x)).is_some_and(|t| c.text_of(t) == "std");
    }
    match recv {
        Some(r) => {
            if callee_decl(c, call).is_some() {
                return false;
            }
            match expr_type(c, r) {
                Some(t) => {
                    BUILTIN_TYPES.contains(&t.as_str())
                        || c.nodes(TypedefDecl).into_iter().any(|td| c.tree.child(td, Name).is_some_and(|x| c.tree.bare_text(x) == t))
                }
                None => !matches!(name.as_str(), "randomize"),
            }
        }
        None => false,
    }
}

const BUILTIN_TYPES: &[&str] = &[
    "bit", "logic", "reg", "int", "integer", "byte", "shortint", "longint", "string", "real", "shortreal", "time",
    "realtime", "event", "chandle",
];

/// The receiver's type is the built-in class `semaphore`.
pub(crate) fn is_semaphore(c: &Cst, recv: NodeId) -> bool {
    expr_type(c, recv).is_some_and(|t| t == "semaphore")
}

/// `swe::isSemaphoreGet`: `get` or `try_get` on a semaphore.
pub(crate) fn is_semaphore_get(c: &Cst, call: NodeId) -> bool {
    match call_parts(c, call) {
        Some((_, name, Some(r))) if name == "get" || name == "try_get" => is_semaphore(c, r),
        _ => false,
    }
}

/// An expression statement's expression with a `void'( )` cast taken off.
pub(crate) fn stmt_call(c: &Cst, stmt: NodeId) -> Option<NodeId> {
    let mut e = *kids(c, stmt).iter().find(|&&x| !matches!(c.kind(x), StmtLabel | Attribute))?;
    while c.kind(e) == CastExpr {
        let k = kids(c, e);
        if k.first().is_some_and(|&x| c.tree.bare_text(x) == "void") && k.len() == 2 {
            e = k[1];
        } else {
            break;
        }
    }
    (c.kind(e) == CallExpr).then_some(e)
}

// ---- symbols written and read (swe::writtenSymbols / referencedSymbols) ----

/// A symbol as the C++ rules key it: a resolved declaration, or a name
/// this lookup cannot resolve; members of an interface or object read as
/// `a.m` are keyed by member name.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub(crate) enum Sym {
    Decl(u32),
    Name(String),
    Member(String),
}

pub(crate) fn name_sym(c: &Cst, e: NodeId) -> Option<(Sym, String, Option<Decl>)> {
    let t = c.first_tok(e)?;
    let name = c.text_of(t);
    if matches!(name, "this" | "super" | "null" | "$") || name.starts_with('$') {
        return None;
    }
    match lookup(c, e, name) {
        Some(d) => Some((Sym::Decl(d.node.0), name.to_string(), Some(d))),
        None => Some((Sym::Name(name.to_string()), name.to_string(), None)),
    }
}

/// `swe::rootSymbol`: `x` of `x`, `x[i]`, `x.f`.
pub(crate) fn root_sym(c: &Cst, e: NodeId) -> Option<(Sym, String, Option<Decl>)> {
    let mut e = e;
    for _ in 0..64 {
        match c.kind(e) {
            NameExpr => return name_sym(c, e),
            SelectExpr | MemberExpr => e = *kids(c, e).first()?,
            _ => return None,
        }
    }
    None
}

const ASSIGN_OPS: &[&str] = &["=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=", "<<<=", ">>>="];

/// An assignment (`=`, `op=`, or a non-blocking `<=` statement): its left side.
pub(crate) fn assign_lhs(c: &Cst, n: NodeId) -> Option<NodeId> {
    if c.kind(n) != BinaryExpr {
        return None;
    }
    let op = c.keyword(n).map(|t| c.text_of(t))?;
    let is = ASSIGN_OPS.contains(&op) || (op == "<=" && c.parent(n).is_some_and(|p| c.kind(p) == ExprStmt));
    if is {
        kids(c, n).first().copied()
    } else {
        None
    }
}

pub(crate) fn is_incdec(c: &Cst, n: NodeId) -> Option<NodeId> {
    if !matches!(c.kind(n), PrefixExpr | PostfixExpr) {
        return None;
    }
    let op = c.toks(n).map(|t| c.text_of(t)).find(|t| matches!(*t, "++" | "--"))?;
    let _ = op;
    kids(c, n).first().copied()
}

/// The direction of each formal of a routine declaration (sticky, as
/// written): input, output, inout, ref or const ref.
fn formal_dirs(c: &Cst, decl: NodeId) -> Vec<&'static str> {
    let Some(pl) = c.tree.child(decl, PortList) else { return Vec::new() };
    let mut out = Vec::new();
    let mut dir = "input";
    for p in c.tree.children(pl).filter(|&p| c.kind(p) == Port) {
        let toks = tok_texts(c, p);
        if toks.contains(&"const") && toks.contains(&"ref") {
            dir = "const ref";
        } else if let Some(d) = toks.iter().find(|t| matches!(**t, "input" | "output" | "inout" | "ref")) {
            dir = match *d {
                "input" => "input",
                "output" => "output",
                "inout" => "inout",
                _ => "ref",
            };
        }
        let n = c.tree.children(p).filter(|&d| c.kind(d) == Declarator).count().max(1);
        for _ in 0..n {
            out.push(dir);
        }
    }
    out
}

pub(crate) fn call_args(c: &Cst, call: NodeId) -> Vec<NodeId> {
    let Some(al) = c.tree.child(call, ArgList) else { return Vec::new() };
    c.tree.children(al).filter(|&a| c.kind(a) == Arg).filter_map(|a| kids(c, a).last().copied()).collect()
}

const MUTATING: &[&str] = &["$cast", "$sscanf", "$fscanf", "$fgets", "$fread", "$value$plusargs", "randomize", "srandom"];
const CONTAINER: &[&str] =
    &["push_back", "push_front", "pop_back", "pop_front", "insert", "delete", "sort", "rsort", "reverse", "shuffle"];

/// Every symbol a node writes: assigned, incremented, passed to an output
/// or ref argument, or changed by a mutating system call (and, with
/// `containers`, an array method). In walk order, each once.
pub(crate) fn written(c: &Cst, n: NodeId, containers: bool) -> Vec<(Sym, String, Option<Decl>)> {
    let mut out: Vec<(Sym, String, Option<Decl>)> = Vec::new();
    let mut push = |x: Option<(Sym, String, Option<Decl>)>| {
        if let Some(x) = x {
            if !out.iter().any(|y| y.0 == x.0) {
                out.push(x);
            }
        }
    };
    for x in subtree(c, n) {
        if let Some(l) = assign_lhs(c, x) {
            push(root_sym(c, l));
        } else if let Some(o) = is_incdec(c, x) {
            push(root_sym(c, o));
        } else if c.kind(x) == CallExpr {
            let Some((_, name, recv)) = call_parts(c, x) else { continue };
            if is_system_call(c, x) {
                if MUTATING.contains(&name.as_str()) || (containers && CONTAINER.contains(&name.as_str())) {
                    if let Some(r) = recv {
                        push(root_sym(c, r));
                    }
                    for a in call_args(c, x) {
                        push(root_sym(c, a));
                    }
                }
            } else if let Some(d) = callee_decl(c, x) {
                let dirs = formal_dirs(c, d);
                for (i, a) in call_args(c, x).into_iter().enumerate() {
                    match dirs.get(i) {
                        Some(&"output") | Some(&"inout") | Some(&"ref") => push(root_sym(c, a)),
                        _ => {}
                    }
                }
                if name == "randomize" {
                    if let Some(r) = recv {
                        push(root_sym(c, r));
                    }
                }
            } else if name == "randomize" {
                // Only on an object of a known class (an unknown type is an error).
                if let Some(r) = recv {
                    if expr_type(c, r).is_some_and(|t| find_class(c, &t, x).is_some()) {
                        push(root_sym(c, r));
                    }
                }
            }
        }
    }
    out
}

/// `swe::referencedSymbols`: names and members an expression reads.
fn referenced(c: &Cst, e: NodeId) -> HashSet<Sym> {
    let mut out = HashSet::new();
    for x in subtree(c, e) {
        match c.kind(x) {
            NameExpr => {
                if let Some((s, _, _)) = name_sym(c, x) {
                    out.insert(s);
                }
            }
            MemberExpr => {
                let is_callee = c.parent(x).is_some_and(|p| c.kind(p) == CallExpr && kids(c, p).first() == Some(&x));
                if !is_callee {
                    out.insert(Sym::Member(c.last_text(x).to_string()));
                }
            }
            _ => {}
        }
    }
    out
}

// ---- SV-CNC-002 ----

fn circular_wait(c: &Cst, s: &mut Sink) {
    for f in c.nodes(ParBlock) {
        let n = times(c, f);
        let mut seqs: Vec<Vec<(String, Loc)>> = Vec::new();
        for t in items(c, f) {
            let mut locks = Vec::new();
            for call in calls_in(c, t) {
                let Some((_, name, recv)) = call_parts(c, call) else { continue };
                if name != "get" {
                    continue;
                }
                // slang gives the receiver no syntax: read it off the call's text.
                let _ = recv;
                let text = slang_text(c, call);
                let sem = match text.find(".get") {
                    Some(dot) => {
                        let st = text[..dot].rfind(' ').map_or(0, |i| i + 1);
                        text[st..dot].to_string()
                    }
                    None => String::new(),
                };
                let sem = sem.trim_matches([' ', '\t', '\n', '\r']).to_string();
                if !sem.is_empty() {
                    if let Some(l) = start(c, call) {
                        locks.push((sem, l));
                    }
                }
            }
            if locks.len() >= 2 {
                seqs.push(locks);
            }
        }
        for t1 in 0..seqs.len() {
            for t2 in t1 + 1..seqs.len() {
                let (s1, s2) = (&seqs[t1], &seqs[t2]);
                for i in 0..s1.len() - 1 {
                    let (a, b) = (&s1[i].0, &s1[i + 1].0);
                    if a == b {
                        continue;
                    }
                    for j in 0..s2.len() - 1 {
                        if *a == s2[j + 1].0 && *b == s2[j].0 {
                            add_n(
                                s,
                                n,
                                "SV-CNC-002",
                                "CircularThreadWaitDeadlock",
                                Severity::Error,
                                format!(
                                    "Circular lock acquisition order detected between concurrent fork threads: \
                                     Thread 1 acquires '{a}' before '{b}', while Thread 2 acquires '{b}' before '{a}'. \
                                     This creates a circular wait dependency and guaranteed simulation deadlock. Enforce a global lock acquisition hierarchy."
                                ),
                                s2[j + 1].1,
                            );
                        }
                    }
                }
            }
        }
    }
}

// ---- SV-CNC-003 ----

/// The statement (with nested ones) contains a `break` or `return`.
fn exits(c: &Cst, st: NodeId) -> bool {
    subtree(c, st).into_iter().any(|x| match c.kind(x) {
        ReturnStmt => true,
        JumpStmt => c.keyword(x).is_some_and(|k| c.text_of(k) == "break"),
        _ => false,
    })
}

/// Calls the C++ rules take for a blocking task call: a user task, or one
/// this lookup cannot resolve (slang would know it).
fn is_task_call(c: &Cst, call: NodeId) -> bool {
    if is_system_call(c, call) {
        return false;
    }
    match callee_decl(c, call) {
        Some(d) => c.kind(d) == TaskDecl || (c.kind(d) == MethodPrototype && has_kw(c, d, "task")),
        None => !builtin_function(c, call),
    }
}

/// Methods of built-in classes and types that are functions.
fn builtin_function(c: &Cst, call: NodeId) -> bool {
    let Some((_, name, recv)) = call_parts(c, call) else { return false };
    let ty = recv.and_then(|r| expr_type(c, r));
    match ty.as_deref() {
        Some("semaphore") => matches!(name.as_str(), "try_get" | "put" | "new"),
        Some("mailbox") => matches!(name.as_str(), "try_get" | "try_put" | "try_peek" | "num" | "new"),
        Some("process") => !matches!(name.as_str(), "await"),
        _ => matches!(name.as_str(), "randomize" | "new" | "srandom" | "get_randstate" | "set_randstate" | "rand_mode" | "constraint_mode"),
    }
}

fn polling_loop(c: &Cst, s: &mut Sink) {
    for (r, n) in user_routines(c) {
        let where_ = format!("'{}'", routine_name(c, r));
        for l in subtree(c, r) {
            if !is_wait_loop(c, l) {
                continue;
            }
            let Some(body) = loop_body(c, l) else { continue };
            let (mut fixed, mut real) = (false, false);
            let mut exit_conds = Vec::new();
            for x in subtree(c, body) {
                match c.kind(x) {
                    TimedStmt => {
                        if let Some(t) = timing_of(c, x) {
                            if timing_kind(c, t) == "#" {
                                if is_fixed_delay(c, t) {
                                    fixed = true;
                                } else {
                                    real = true;
                                }
                            } else {
                                real = true;
                            }
                        }
                    }
                    WaitStmt => {
                        if c.keyword(x).is_some_and(|k| c.text_of(k) == "wait_order")
                            || !c.toks(x).any(|t| c.text_of(t) == "fork")
                        {
                            real = true;
                        }
                    }
                    CallExpr => {
                        if is_task_call(c, x) {
                            real = true;
                        }
                    }
                    IfStmt => {
                        let (cond, then, _) = if_parts(c, x);
                        if then.is_some_and(|t| exits(c, t)) {
                            if let Some(cd) = cond {
                                exit_conds.push(cd);
                            }
                        }
                    }
                    _ => {}
                }
            }
            if !fixed || real {
                continue;
            }
            let conds = if c.kind(l) == DoWhileStmt || loop_kw(c, l) == "while" {
                let Some(cd) = loop_cond(c, l) else { continue };
                if is_constant(c, cd) {
                    continue;
                }
                vec![cd]
            } else {
                exit_conds
            };
            if conds.is_empty() {
                continue;
            }
            let w: HashSet<Sym> = written(c, body, true).into_iter().map(|x| x.0).collect();
            for cd in conds {
                if referenced(c, cd).iter().any(|x| w.contains(x)) {
                    continue;
                }
                let t = norm_text(c, cd);
                if let Some(loc) = start(c, l) {
                    add_n(
                        s,
                        n,
                        "SV-CNC-003",
                        "PollingLoop",
                        Severity::Warning,
                        format!(
                            "Loop in {where_} polls '{t}' with a fixed delay. Polling ties the check to the timescale, misses short pulses and slows simulation; wait for the change instead with wait({t}), @(...) on the signal or clocking block, or an event."
                        ),
                        loc,
                    );
                }
                break;
            }
        }
    }
}

// ---- SV-CNC-004 ----

/// A fork's branches: its statements less declarations and empty ones.
fn branches(c: &Cst, f: NodeId) -> Vec<NodeId> {
    let it = items(c, f);
    if it.len() == 1 {
        return it;
    }
    it.into_iter().filter(|&x| !matches!(c.kind(x), DataDecl | EmptyStmt)).collect()
}

fn uses_semaphore(c: &Cst, n: NodeId) -> bool {
    calls_in(c, n).into_iter().any(|x| is_semaphore_get(c, x))
}

fn property_writes(c: &Cst, n: NodeId) -> Vec<(Sym, String)> {
    written(c, n, false)
        .into_iter()
        .filter(|x| x.2.is_some_and(|d| d.kind == DeclKind::Property))
        .map(|x| (x.0, x.1))
        .collect()
}

/// A fork's statements as slang's block body: the list when there are two
/// or more, else the one statement.
fn fork_body_writes(c: &Cst, f: NodeId) -> Vec<(Sym, String)> {
    let mut out: Vec<(Sym, String)> = Vec::new();
    for it in items(c, f) {
        for w in property_writes(c, it) {
            if !out.iter().any(|x| x.0 == w.0) {
                out.push(w);
            }
        }
    }
    out
}

fn shared_write(c: &Cst, s: &mut Sink) {
    for (r, n) in user_routines(c) {
        let where_ = format!("'{}'", routine_name(c, r));
        let report = |s: &mut Sink, f: NodeId, how: String, name: &str| {
            if let Some(loc) = start(c, f) {
                add_n(
                    s,
                    n,
                    "SV-CNC-004",
                    "SharedWriteFromForks",
                    Severity::Warning,
                    format!(
                        "Property '{name}' {how} in {where_}, with no semaphore around the writes. The final value depends on thread scheduling; give each thread its own variable, or guard the writes with a semaphore."
                    ),
                    loc,
                );
            }
        };
        for f in subtree(c, r).into_iter().filter(|&x| c.kind(x) == ParBlock) {
            // Branches written by two or more.
            let br = branches(c, f);
            if br.len() >= 2 {
                let mut writers: BTreeMap<Sym, (usize, String)> = BTreeMap::new();
                for b in &br {
                    if uses_semaphore(c, *b) {
                        continue;
                    }
                    for (sym, name) in property_writes(c, *b) {
                        writers.entry(sym).or_insert((0, name)).0 += 1;
                    }
                }
                for (_, (count, name)) in writers {
                    if count >= 2 {
                        report(s, f, format!("is written by {count} branches of the same fork"), &name);
                    }
                }
            }
            // A join_none thread and the statements after it.
            if join_kind(c, f) != "join_none" {
                continue;
            }
            let Some(p) = c.parent(f) else { continue };
            if !matches!(c.kind(p), SeqBlock | ParBlock | Body) {
                continue;
            }
            let list = items(c, p);
            if list.len() < 2 {
                continue;
            }
            if uses_semaphore(c, f) {
                continue;
            }
            let mine = fork_body_writes(c, f);
            if mine.is_empty() {
                continue;
            }
            let ix = list.iter().position(|&x| x == f).unwrap_or(list.len());
            let mut after = HashSet::new();
            let mut guarded = false;
            for &j in &list[ix + 1..] {
                if uses_semaphore(c, j) {
                    guarded = true;
                    break;
                }
                for (sym, _) in property_writes(c, j) {
                    after.insert(sym);
                }
            }
            if guarded {
                continue;
            }
            for (sym, name) in mine {
                if after.contains(&sym) {
                    report(s, f, "is written by a join_none thread and by the code that forked it".into(), &name);
                }
            }
        }
    }
}

// ---- SV-CON-006 ----

fn proc_kind<'a>(c: &'a Cst, p: NodeId) -> &'a str {
    c.keyword(p).map_or("", |k| c.text_of(k))
}

fn trigger_in_comb(c: &Cst, s: &mut Sink) {
    for p in c.nodes(ProceduralBlock) {
        if proc_kind(c, p) != "always_comb" {
            continue;
        }
        let n = times(c, p);
        for x in subtree(c, p) {
            if c.kind(x) == EventTrigger {
                if let Some(loc) = start(c, x) {
                    add_n(
                        s,
                        n,
                        "SV-CON-006",
                        "EventTriggerInAlwaysComb",
                        Severity::Error,
                        "Event trigger inside 'always_comb' block causes simulation delta-cycle oscillation and simulator lockup. Per IEEE 1800 § 9.2.2.2, combinational blocks must be purely functional and sensitive only to signal reads; triggering events produces infinite scheduling loops.".into(),
                        loc,
                    );
                }
            }
        }
    }
}

// ---- SV-CON-008 ----

fn try_get_unchecked(c: &Cst, s: &mut Sink) {
    for st in c.nodes(ExprStmt) {
        let Some(call) = stmt_call(c, st) else { continue };
        let Some((_, name, recv)) = call_parts(c, call) else { continue };
        if name != "try_get" {
            continue;
        }
        let Some(r) = recv else { continue };
        let ty = expr_type(c, r).unwrap_or_default();
        let mut mutex = ty.contains("semaphore") || ty.contains("mutex") || ty.contains("lock");
        let target = match c.kind(r) {
            NameExpr => c.first_tok(r).map(|t| c.text_of(t).to_string()).unwrap_or_default(),
            MemberExpr => c.last_text(r).to_string(),
            _ => slang_trimmed(c, r),
        };
        if target.contains("sem") || target.contains("mutex") || target.contains("lock") {
            mutex = true;
        }
        if !mutex {
            continue;
        }
        if let Some(loc) = start(c, call) {
            add_n(
                s,
                times(c, st),
                "SV-CON-008",
                "MutexTryGetWithoutCheck",
                Severity::Warning,
                format!(
                    "Non-blocking semaphore acquisition '{target}.try_get()' called without verifying its return code. \
                     In SystemVerilog (IEEE 1800 § 15.3), try_get() returns 0 when the requested keys are unavailable. \
                     Discarding the return value causes the thread to execute critical section code without acquiring the mutual exclusion lock, \
                     risking catastrophic race conditions and data corruption. Wrap the call in 'if ({target}.try_get(...))'."
                ),
                loc,
            );
        }
    }
}

// ---- SV-CON-009 ----

fn wait_fork_without_fork(c: &Cst, s: &mut Sink) {
    for k in [FunctionDecl, TaskDecl] {
        for r in c.nodes(k) {
            let name = routine_name(c, r);
            let all = subtree(c, r);
            if all.iter().any(|&x| c.kind(x) == ParBlock) {
                continue;
            }
            for x in all {
                if c.kind(x) == WaitStmt && c.toks(x).any(|t| c.text_of(t) == "fork") {
                    if let Some(loc) = start(c, x) {
                        s.add(
                            "SV-CON-009",
                            "WaitForkWithoutFork",
                            Severity::Warning,
                            format!(
                                "Routine '{name}' executes 'wait fork;' but contains no parallel 'fork' blocks. \
                                 In SystemVerilog (IEEE 1800 § 9.6.1), 'wait fork' blocks execution until all immediate child processes \
                                 spawned by the current process terminate. Calling 'wait fork' in a routine that spawned zero child threads \
                                 either executes as a useless no-op or causes unintended hangs by waiting on unrelated threads spawned by caller processes."
                            ),
                            loc,
                        );
                    }
                }
            }
        }
    }
}

// ---- SV-CON-011 ----

/// The identifiers slang keeps as `IdentifierName` under a node: names and
/// members, but not a name that a select follows (`a` of `a[i]`, which is
/// an `IdentifierSelectName`), in source order.
pub(crate) fn identifier_names(c: &Cst, n: NodeId) -> Vec<String> {
    let ident = |s: &str| s.starts_with(|ch: char| ch.is_ascii_alphabetic() || ch == '_') && !matches!(s, "this" | "super" | "null");
    let mut res: Vec<(u32, String)> = Vec::new();
    for x in subtree(c, n) {
        let selected = c.parent(x).is_some_and(|p| c.kind(p) == SelectExpr && kids(c, p).first() == Some(&x));
        if selected {
            continue;
        }
        let t = match c.kind(x) {
            NameExpr => c.first_tok(x),
            // The member is the last name token after the base.
            MemberExpr | ScopedExpr => c.toks(x).filter(|&t| ident(c.text_of(t))).last(),
            _ => None,
        };
        if let Some(t) = t {
            if ident(c.text_of(t)) {
                res.push((t, c.text_of(t).to_string()));
            }
        }
    }
    res.sort_by_key(|x| x.0);
    res.into_iter().map(|x| x.1).collect()
}

fn wait_in_loop(c: &Cst, s: &mut Sink) {
    for l in subtree(c, c.tree.root) {
        if !(c.kind(l) == DoWhileStmt || (c.kind(l) == LoopStmt && loop_kw(c, l) != "foreach")) {
            continue;
        }
        let all = subtree(c, l);
        let waits: Vec<NodeId> = all
            .iter()
            .copied()
            .filter(|&x| c.kind(x) == WaitStmt && c.keyword(x).is_some_and(|k| c.text_of(k) == "wait") && !c.toks(x).any(|t| c.text_of(t) == "fork"))
            .collect();
        if waits.is_empty() {
            continue;
        }
        let mut modified: HashSet<String> = HashSet::new();
        for &x in &all {
            if let Some(lhs) = assign_lhs(c, x) {
                modified.extend(identifier_names(c, lhs));
            } else if let Some(o) = is_incdec(c, x) {
                modified.extend(identifier_names(c, o));
            }
        }
        for w in waits {
            let Some(cond) = kids(c, w).into_iter().find(|&x| !matches!(c.kind(x), StmtLabel | Attribute)) else { continue };
            let ids = identifier_names(c, cond);
            if ids.is_empty() || ids.iter().any(|i| modified.contains(i)) {
                continue;
            }
            let Some(order) = std_unordered_order(&ids) else { continue };
            let list = order.iter().map(|i| format!("'{i}'")).collect::<Vec<_>>().join(", ");
            let Some(kw) = c.tok(w, "wait") else { continue };
            s.add(
                "SV-CON-011",
                "WaitModifiedInLoop",
                Severity::Warning,
                format!(
                    "Condition variables ({list}) in 'wait(...)' statement are not modified or assigned anywhere inside the loop body. \
                     In SystemVerilog concurrency guidelines (IEEE 1800 § 9.4.4), waiting inside a loop without mutating condition variables risks \
                     zero-delay infinite spinning or unblocked iteration once the condition becomes true."
                ),
                c.loc(kw),
            );
        }
    }
}

// ---- SV-CON-014 ----

/// Methods of a class declared in it (an out-of-block body counts).
fn own_methods(c: &Cst, cls: NodeId) -> Vec<NodeId> {
    class_methods(c, cls)
}

fn named_blocks(c: &Cst, r: NodeId) -> HashSet<String> {
    let mut out = HashSet::new();
    for x in subtree(c, r) {
        if matches!(c.kind(x), SeqBlock | ParBlock) {
            for l in kids(c, x) {
                if matches!(c.kind(l), StmtLabel | EndLabel) {
                    if let Some(t) = c.toks(l).map(|t| c.text_of(t)).find(|t| *t != ":") {
                        out.insert(t.to_string());
                    }
                }
            }
        }
    }
    out
}

fn disable_in_class(c: &Cst, s: &mut Sink) {
    for cls in user_classes(c) {
        let cname = decl_name(c, cls);
        for m in own_methods(c, cls) {
            let blocks = named_blocks(c, m);
            for x in subtree(c, m) {
                if c.kind(x) != DisableStmt {
                    continue;
                }
                let toks = tok_texts(c, x);
                if toks.get(1) == Some(&"fork") {
                    continue;
                }
                let target = kids(c, x).first().map(|&n| c.tree.bare_text(n)).unwrap_or_else(|| {
                    toks.iter().skip(1).filter(|t| **t != ";").copied().collect::<String>()
                });
                let last = target.rsplit('.').next().unwrap_or("").to_string();
                let is_block = blocks.contains(&last);
                let is_task = !is_block && {
                    let chain = class_chain(c, cls);
                    chain.iter().any(|&k| {
                        kids(c, k).into_iter().any(|m2| {
                            (c.kind(m2) == TaskDecl || (c.kind(m2) == MethodPrototype && has_kw(c, m2, "task")))
                                && last_name(c, m2) == last
                        })
                    }) || c.nodes(TaskDecl).into_iter().any(|t| decl_name(c, t) == last && enclosing_class(c, t).is_none())
                };
                if !is_block && !is_task {
                    continue;
                }
                let Some(loc) = start(c, x) else { continue };
                s.add(
                    "SV-CON-014",
                    "DisableNamedBlockInClass",
                    Severity::Warning,
                    format!(
                        "'disable {last}' in class '{cname}' stops that {} in every object of the class, not just this one. \
                         Keep a process handle (process::self()) and call kill() on it, \
                         or use break/continue/return to leave a loop or task.",
                        if is_block { "block" } else { "task" }
                    ),
                    loc,
                );
            }
        }
    }
}

// ---- SV-CON-015 ----

/// `swe::classRole` reduced to the two roles SV-CON-015 needs.
fn role_of(c: &Cst, cls: NodeId) -> &'static str {
    for n in base_names(c, cls) {
        if !n.starts_with("uvm_") {
            continue;
        }
        return match n.as_str() {
            "uvm_driver" | "uvm_push_driver" => "driver",
            "uvm_sequence" | "uvm_sequence_base" | "uvm_reg_sequence" => "sequence",
            "uvm_monitor" | "uvm_agent" | "uvm_scoreboard" | "uvm_subscriber" | "uvm_env" | "uvm_test" | "uvm_report_catcher"
            | "uvm_sequence_item" | "uvm_transaction" | "uvm_component" | "uvm_root" | "uvm_object" | "uvm_void"
            | "uvm_push_sequencer" | "uvm_mem" | "uvm_vreg" => "other",
            x if x.starts_with("uvm_sequencer") || x.starts_with("uvm_reg") || x.starts_with("uvm_callback") || x.starts_with("uvm_tlm") => "other",
            _ => continue,
        };
    }
    "other"
}

/// The virtual interface type a declaration has: the interface's name.
fn vif_type(c: &Cst, d: Decl) -> Option<String> {
    let dt = d.dtype?;
    let all: Vec<&str> = c.tree.token_range(dt).into_iter().map(|t| c.text_of(t)).collect();
    let toks = all;
    if toks.first() == Some(&"virtual") {
        return toks.iter().skip(1).find(|t| **t != "interface").map(|t| t.to_string());
    }
    // A typedef of a virtual interface.
    let name = *toks.first()?;
    for td in c.nodes(TypedefDecl) {
        if c.tree.child(td, Name).is_some_and(|x| c.tree.bare_text(x) == name) {
            let dt2 = c.tree.child(td, DataType)?;
            let t2: Vec<&str> = c.tree.token_range(dt2).into_iter().map(|t| c.text_of(t)).collect();
            if t2.first() == Some(&"virtual") {
                return t2.iter().skip(1).find(|t| **t != "interface").map(|t| t.to_string());
            }
        }
    }
    None
}

fn is_clocking(c: &Cst, intf: &str, member: &str) -> bool {
    c.nodes(InterfaceDecl).into_iter().filter(|&i| decl_name(c, i) == intf).any(|i| {
        subtree(c, i).into_iter().any(|x| c.kind(x) == ClockingDecl && decl_name(c, x) == member)
    })
}

fn is_exempt_signal(name: &str) -> bool {
    let n = name.to_lowercase();
    ["clk", "clock", "rst", "reset"].iter().any(|k| n.contains(k))
}

/// The interface signal an expression reads first: `Some("")` for a
/// clocking block, clock or reset; `None` when it reads no virtual
/// interface.
fn vif_signal(c: &Cst, e: NodeId) -> Option<String> {
    let mut stack = vec![e];
    while let Some(x) = stack.pop() {
        if c.kind(x) == MemberExpr {
            let is_callee = c.parent(x).is_some_and(|p| c.kind(p) == CallExpr && kids(c, p).first() == Some(&x));
            if !is_callee {
                let mut root = *kids(c, x).first()?;
                while c.kind(root) == MemberExpr {
                    root = *kids(c, root).first()?;
                }
                if c.kind(root) == NameExpr {
                    if let Some((_, _, Some(d))) = name_sym(c, root) {
                        if let Some(intf) = vif_type(c, d) {
                            // Only the member right on the interface handle.
                            let member = c.last_text(x).to_string();
                            let exempt = is_clocking(c, &intf, &member) || is_exempt_signal(&member);
                            return Some(if exempt { String::new() } else { member });
                        }
                    }
                }
            }
        }
        let k: Vec<NodeId> = c.tree.children(x).collect();
        stack.extend(k.into_iter().rev());
    }
    None
}

fn unguarded_wait(c: &Cst, s: &mut Sink) {
    for cls in user_classes(c) {
        if !matches!(role_of(c, cls), "driver" | "sequence") {
            continue;
        }
        let cname = decl_name(c, cls);
        for m in own_methods(c, cls) {
            if c.kind(m) != TaskDecl {
                continue;
            }
            let mname = routine_name(c, m).to_lowercase();
            if mname.contains("timeout") || mname.contains("watchdog") {
                continue;
            }
            for x in subtree(c, m) {
                let guarded = ancestors(c, x).take_while(|&a| a != m).any(|a| c.kind(a) == ParBlock && join_kind(c, a) == "join_any");
                if guarded {
                    continue;
                }
                let (expr, how) = match c.kind(x) {
                    WaitStmt if c.keyword(x).is_some_and(|k| c.text_of(k) == "wait") && !c.toks(x).any(|t| c.text_of(t) == "fork") => {
                        (kids(c, x).into_iter().find(|&y| !matches!(c.kind(y), StmtLabel | Attribute)), "wait")
                    }
                    TimedStmt => {
                        let Some(t) = timing_of(c, x) else { continue };
                        if timing_kind(c, t) != "@" {
                            continue;
                        }
                        let evs: Vec<NodeId> = kids(c, t).into_iter().filter(|&e| c.kind(e) == EventExpr).collect();
                        if evs.len() > 1
                            && evs.iter().any(|&e| {
                                let ex = kids(c, e).first().copied();
                                !ex.and_then(|ex| vif_signal(c, ex)).is_some_and(|sg| !sg.is_empty())
                            })
                        {
                            continue;
                        }
                        (evs.first().and_then(|&e| kids(c, e).first().copied()), "@")
                    }
                    _ => continue,
                };
                let Some(expr) = expr else { continue };
                let Some(sig) = vif_signal(c, expr) else { continue };
                if sig.is_empty() {
                    continue;
                }
                let Some(loc) = start(c, x) else { continue };
                s.add(
                    "SV-CON-015",
                    "UnguardedBlockingWait",
                    Severity::Warning,
                    format!(
                        "{how} on interface signal '{sig}' in '{cname}' has no timeout. If the DUT never answers, the test hangs until the global \
                         timeout with no hint of which agent stalled. Race it against a delay in \
                         fork ... join_any, or call a wait-with-timeout helper."
                    ),
                    loc,
                );
            }
        }
    }
}

// ---- SV-DLY-003 ----

fn repeat_without_timing(c: &Cst, s: &mut Sink) {
    for t in c.nodes(TaskDecl) {
        let name = routine_name(c, t);
        for l in subtree(c, t) {
            if c.kind(l) != LoopStmt || loop_kw(c, l) != "repeat" {
                continue;
            }
            let Some(body) = loop_body(c, l) else { continue };
            let timed = subtree(c, body).into_iter().any(|x| match c.kind(x) {
                TimedStmt | ParBlock => true,
                WaitStmt => c.keyword(x).is_some_and(|k| c.text_of(k) == "wait") && !c.toks(x).any(|t| c.text_of(t) == "fork"),
                _ => false,
            });
            if timed {
                continue;
            }
            let text = slang_text(c, body);
            if text.contains('@') || text.contains('#') || text.contains("wait") {
                continue;
            }
            let Some(kw) = c.tok(l, "repeat") else { continue };
            s.add(
                "SV-DLY-003",
                "RepeatLoopWithoutTiming",
                Severity::Warning,
                format!(
                    "Repeat loop inside task '{name}()' contains no time-consuming statements (#, @, wait). \
                     In verification testbenches, tasks operate across simulation time; \
                     a repeat loop without clock or delay synchronization spins instantaneously in a single delta cycle and frequently indicates a missing '@(posedge clk)' or clocking block synchronization."
                ),
                c.loc(kw),
            );
        }
    }
}

// ---- SV-DLY-004 ----

fn zero_time_forever(c: &Cst, s: &mut Sink) {
    for l in c.nodes(LoopStmt) {
        if loop_kw(c, l) != "forever" {
            continue;
        }
        let Some(body) = loop_body(c, l) else { continue };
        let found = subtree(c, body).into_iter().any(|x| match c.kind(x) {
            TimedStmt | WaitStmt | DisableStmt | ReturnStmt => true,
            JumpStmt => c.keyword(x).is_some_and(|k| c.text_of(k) == "break"),
            CallExpr => {
                let name = call_parts(c, x).map(|p| p.1).unwrap_or_default();
                if name.starts_with('$') {
                    matches!(name.as_str(), "$finish" | "$stop" | "$fatal")
                } else {
                    is_task_call(c, x)
                }
            }
            _ => false,
        });
        if found {
            continue;
        }
        if let Some(loc) = start(c, l) {
            add_n(
                s,
                times(c, l),
                "SV-DLY-004",
                "ZeroTimeForeverLoop",
                Severity::Error,
                "forever loop with no timing control (#, @, wait), no task call and no way out (break, \
                 return, disable). It runs forever in zero simulation time and hangs the simulator. \
                 Wait on a clock or event inside the loop, or call the blocking task it is meant to wait in."
                    .into(),
                loc,
            );
        }
    }
}

// ---- SV-DRV-001 ----

fn driving_during_reset(c: &Cst, s: &mut Sink) {
    for t in c.nodes(TaskDecl) {
        let Some(kw) = c.tok(t, "task") else { continue };
        let Some((_, last)) = c.tree.token_bounds(t) else { continue };
        let text = span_text(c, kw, last);
        let drives = (text.contains("vif.") || text.contains("intf.")) && text.contains("<=");
        if !drives {
            continue;
        }
        let mut clean = text.clone();
        while let Some(p) = clean.find("//") {
            let eol = clean[p..].find('\n').map_or(clean.len(), |e| p + e);
            clean.replace_range(p..eol, "");
        }
        let lower = clean.to_lowercase();
        if lower.contains("rst") || lower.contains("reset") {
            continue;
        }
        let Some(nm) = c.tree.child(t, Name) else { continue };
        let Some(ft) = c.first_tok(nm) else { continue };
        let name = c.text_of(ft);
        s.add(
            "SV-DRV-001",
            "DrivingDuringReset",
            Severity::Warning,
            format!(
                "Task '{name}' drives virtual interface signals without an active reset check or synchronization guard. \
                 Driving protocol control signals while hardware reset is asserted causes bus protocol violations \
                 and synchronizer state corruption. Guard signal driving with 'wait(vif.rst_n == 1)' or 'if (!vif.rst_n)'."
            ),
            c.loc(ft),
        );
    }
}

// ---- SV-EVT-001 ----

fn triggered_wait_loop(c: &Cst, s: &mut Sink) {
    for l in subtree(c, c.tree.root) {
        if !is_wait_loop(c, l) {
            continue;
        }
        let Some(body) = loop_body(c, l) else { continue };
        let mut advance = false;
        let mut wait_loc = None;
        for x in subtree(c, body) {
            match c.kind(x) {
                TimedStmt => advance = true,
                WaitStmt if c.keyword(x).is_some_and(|k| c.text_of(k) == "wait") && !c.toks(x).any(|t| c.text_of(t) == "fork") => {
                    let cond = kids(c, x).into_iter().find(|&y| !matches!(c.kind(y), StmtLabel | Attribute));
                    if cond.is_some_and(|cd| slang_text(c, cd).contains(".triggered")) {
                        wait_loc = start(c, x);
                    }
                }
                _ => {}
            }
        }
        let Some(loc) = wait_loc else { continue };
        if advance {
            continue;
        }
        add_n(
            s,
            times(c, l),
            "SV-EVT-001",
            "ZeroDelayEventWaitLoop",
            Severity::Error,
            "Loop contains 'wait(event.triggered)' without an intervening simulation time advance (@ or #). \
             In SystemVerilog (IEEE 1800 Sec 15.5.2), 'event.triggered' remains high for the entire current time-step across all delta cycles. \
             Without a time-advancing delay, this loop executes infinitely in the same delta cycle, deadlocking the simulator."
                .into(),
            loc,
        );
    }
}

// ---- SV-FOR-001 ----

fn fork_in_function(c: &Cst, s: &mut Sink) {
    for f in c.nodes(FunctionDecl) {
        let name = routine_name(c, f);
        for b in subtree(c, f) {
            if c.kind(b) != ParBlock {
                continue;
            }
            let Some(kw) = c.tok(b, "fork") else { continue };
            let join = join_kind(c, b);
            s.add(
                "SV-FOR-001",
                "ForkInsideFunction",
                Severity::Warning,
                format!(
                    "Spawning concurrent background process with 'fork ... {join}' inside function '{name}()'. \
                     IEEE 1800 § 13.4.4 requires functions to remain non-time-consuming and instantaneous; \
                     spawning threads from functions creates untracked background execution and risks corrupting local automatic variables after the function returns. \
                     Refactor concurrent thread creation to a task."
                ),
                c.loc(kw),
            );
        }
    }
}

// ---- SV-INT-001 ----

fn blocking_to_interface(c: &Cst, s: &mut Sink) {
    for b in c.nodes(BinaryExpr) {
        let Some(op) = c.keyword(b) else { continue };
        if c.text_of(op) != "=" {
            continue;
        }
        let Some(&l) = kids(c, b).first() else { continue };
        let lhs = slang_trimmed(c, l);
        if lhs.contains(".cb.") || lhs.starts_with("cb.") || lhs.contains("vif.") || lhs.contains("_vif.") {
            s.add(
                "SV-INT-001",
                "BlockingAssignmentToInterface",
                Severity::Error,
                format!(
                    "Blocking assignment '=' used to drive clocking block or interface signal '{lhs}'. In SystemVerilog (IEEE 1800 § 14.16), clocking block and interface signals must be driven using non-blocking clocking drive ('<=') to avoid simulation race conditions with DUT Active-region logic."
                ),
                c.loc(op),
            );
        }
    }
}

// ---- SV-IPC-001 ----

fn try_put_ignored(c: &Cst, s: &mut Sink) {
    for st in c.nodes(ExprStmt) {
        let Some(call) = stmt_call(c, st) else { continue };
        let Some((_, name, recv)) = call_parts(c, call) else { continue };
        if !matches!(name.as_str(), "try_put" | "try_get" | "try_peek") {
            continue;
        }
        if name == "try_get" && recv.is_some_and(|r| expr_type(c, r).is_some_and(|t| t.contains("semaphore"))) {
            continue;
        }
        if let Some(loc) = start(c, call) {
            add_n(
                s,
                times(c, st),
                "SV-IPC-001",
                "TryPutIgnoredReturn",
                Severity::Warning,
                format!(
                    "The return value of non-blocking IPC method '{name}()' is ignored or cast to void. \
                     Non-blocking IPC calls return 1 on success and 0 when the FIFO or mailbox is full or empty. \
                     Discarding the return value causes transactions to be silently lost without warning. \
                     Check the return value with 'if (!fifo.{name}(...))' or use blocking 'put()'."
                ),
                loc,
            );
        }
    }
}

// ---- SV-RSC-001 ----

#[derive(Clone, Copy, PartialEq, Eq)]
enum LockOp {
    None,
    Acquire,
    Release,
}

fn classify(c: &Cst, call: NodeId) -> (LockOp, String) {
    let Some((_, n, recv)) = call_parts(c, call) else { return (LockOp::None, String::new()) };
    if let Some(r) = recv {
        if is_semaphore(c, r) {
            // slang gives the receiver no syntax, so its text is empty.
            let _ = r;
            let key = String::new();
            return match n.as_str() {
                "get" => (LockOp::Acquire, key),
                "put" => (LockOp::Release, key),
                _ => (LockOp::None, key),
            };
        }
    }
    match n.as_str() {
        "lock" | "grab" => (LockOp::Acquire, n),
        "unlock" => (LockOp::Release, "lock".into()),
        "ungrab" => (LockOp::Release, "grab".into()),
        _ => (LockOp::None, String::new()),
    }
}

type Held = BTreeMap<String, i32>;

struct LockHooks<'c, 't> {
    c: &'c Cst<'t>,
    released: BTreeSet<String>,
    acquired_at: HashMap<String, Loc>,
    leaks: Vec<(String, Option<Loc>)>,
    reported: BTreeSet<String>,
}

impl<'c, 't> PathHooks<'c, 't> for LockHooks<'c, 't> {
    type St = Held;

    fn cst(&self) -> &'c Cst<'t> {
        self.c
    }

    fn leaf(&mut self, s: NodeId, st: &mut Held) {
        for call in calls_in(self.c, s) {
            self.apply(call, st);
        }
    }

    fn branch(&mut self, cond: NodeId, taken: bool, st: &mut Held) {
        self.try_get(cond, taken, st)
    }

    fn returned(&mut self, s: NodeId, st: &Held) {
        let at = start(self.c, s);
        self.leak(st, at);
    }
}

impl LockHooks<'_, '_> {
    fn apply(&mut self, call: NodeId, st: &mut Held) {
        let (op, key) = classify(self.c, call);
        match op {
            LockOp::Acquire => {
                *st.entry(key.clone()).or_insert(0) += 1;
                if let Some(l) = start(self.c, call) {
                    self.acquired_at.entry(key).or_insert(l);
                }
            }
            LockOp::Release => {
                if let Some(v) = st.get_mut(&key) {
                    *v -= 1;
                    if *v <= 0 {
                        st.remove(&key);
                    }
                }
            }
            LockOp::None => {}
        }
    }

    fn try_get(&mut self, cond: NodeId, taken: bool, st: &mut Held) {
        let c = self.c;
        let mut e = cond;
        let mut negated = false;
        if c.kind(e) == PrefixExpr && c.keyword(e).is_some_and(|k| c.text_of(k) == "!") {
            negated = true;
            let Some(&x) = kids(c, e).first() else { return };
            e = x;
        }
        while c.kind(e) == ParenExpr {
            let Some(&x) = kids(c, e).first() else { return };
            e = x;
        }
        if c.kind(e) != CallExpr {
            return;
        }
        let Some((_, name, Some(r))) = call_parts(c, e) else { return };
        if name != "try_get" {
            return;
        }
        if taken != negated {
            let _ = r;
            let key = String::new();
            *st.entry(key.clone()).or_insert(0) += 1;
            if let Some(l) = start(c, e) {
                self.acquired_at.entry(key).or_insert(l);
            }
        }
    }

    fn leak(&mut self, st: &Held, at: Option<Loc>) {
        for (key, &count) in st {
            if count <= 0 || !self.released.contains(key) || self.reported.contains(key) {
                continue;
            }
            self.reported.insert(key.clone());
            self.leaks.push((key.clone(), at));
        }
    }
}

/// `swe::PathWalker`'s hooks: a path-sensitive walk over a routine body.
/// Loops run zero or one time, join_none bodies are skipped, and
/// `uvm_fatal / $fatal / $finish end a path.
pub(crate) trait PathHooks<'c, 't> {
    type St: Clone + PartialEq;
    fn cst(&self) -> &'c Cst<'t>;
    /// A statement with no sub-statements.
    fn leaf(&mut self, s: NodeId, st: &mut Self::St);
    /// Refines a path's state on a condition.
    fn branch(&mut self, cond: NodeId, taken: bool, st: &mut Self::St);
    /// A path leaves through `return`.
    fn returned(&mut self, s: NodeId, st: &Self::St);
}

pub(crate) struct Out<S> {
    pub(crate) fall: Vec<S>,
    brk: Vec<S>,
    cont: Vec<S>,
}

impl<S> Default for Out<S> {
    fn default() -> Self {
        Out { fall: Vec::new(), brk: Vec::new(), cont: Vec::new() }
    }
}

const MAX_STATES: usize = 32;

fn merge<S: Clone + PartialEq>(into: &mut Vec<S>, from: &[S]) {
    for s in from {
        if into.len() >= MAX_STATES {
            return;
        }
        if !into.contains(s) {
            into.push(s.clone());
        }
    }
}

/// `uvm_report_enabled(...)` and friends are the only calls in the
/// condition: the guard a `uvm_info/`uvm_fatal expands to.
pub(crate) fn is_report_guard(c: &Cst, cond: NodeId) -> bool {
    let calls = calls_in(c, cond);
    let mut report = false;
    for call in calls {
        let n = call_parts(c, call).map(|p| p.1).unwrap_or_default();
        if matches!(n.as_str(), "uvm_report_enabled" | "get_report_verbosity_level" | "get_report_action" | "uvm_get_report_object") {
            report = true;
        } else {
            return false;
        }
    }
    report
}

fn stops(c: &Cst, s: NodeId) -> bool {
    if c.kind(s) != ExprStmt {
        return false;
    }
    let Some(call) = stmt_call(c, s) else { return false };
    let n = call_parts(c, call).map(|p| p.1).unwrap_or_default();
    matches!(n.as_str(), "uvm_report_fatal" | "$fatal" | "$finish" | "$exit")
}

pub(crate) struct Walker<'h, H> {
    pub(crate) h: &'h mut H,
}

impl<'c, 't: 'c, H: PathHooks<'c, 't>> Walker<'_, H> {
    pub(crate) fn list(&mut self, stmts: &[NodeId], input: Vec<H::St>) -> Out<H::St> {
        let mut out = Out::default();
        let mut cur = input;
        for &st in stmts {
            let o = self.walk(st, cur);
            merge(&mut out.brk, &o.brk);
            merge(&mut out.cont, &o.cont);
            cur = o.fall;
            if cur.is_empty() {
                break;
            }
        }
        out.fall = cur;
        out
    }

    fn block(&mut self, b: NodeId, input: Vec<H::St>) -> Out<H::St> {
        let it = items(self.h.cst(), b);
        self.list(&it, input)
    }

    fn walk(&mut self, s: NodeId, input: Vec<H::St>) -> Out<H::St> {
        let c = self.h.cst();
        let mut out = Out::default();
        if input.is_empty() {
            return out;
        }
        match c.kind(s) {
            SeqBlock => self.block(s, input),
            ParBlock => {
                if join_kind(c, s) == "join_none" {
                    out.fall = input;
                    return out;
                }
                self.block(s, input)
            }
            IfStmt => {
                let (cond, then, els) = if_parts(c, s);
                let Some(cond) = cond else { return out };
                if is_report_guard(c, cond) {
                    return match then {
                        Some(t) => self.walk(t, input),
                        None => out,
                    };
                }
                let mut t = input.clone();
                let mut f = input;
                for st in t.iter_mut() {
                    self.h.branch(cond, true, st);
                }
                for st in f.iter_mut() {
                    self.h.branch(cond, false, st);
                }
                let ot = match then {
                    Some(x) => self.walk(x, t),
                    None => Out { fall: t, ..Default::default() },
                };
                let of = match els {
                    Some(x) => self.walk(x, f),
                    None => Out { fall: f, ..Default::default() },
                };
                merge(&mut out.fall, &ot.fall);
                merge(&mut out.fall, &of.fall);
                merge(&mut out.brk, &ot.brk);
                merge(&mut out.brk, &of.brk);
                merge(&mut out.cont, &ot.cont);
                merge(&mut out.cont, &of.cont);
                out
            }
            CaseStmt => {
                let (arms, default) = case_arms(c, s);
                for a in arms {
                    let o = self.walk(a, input.clone());
                    merge(&mut out.fall, &o.fall);
                    merge(&mut out.brk, &o.brk);
                    merge(&mut out.cont, &o.cont);
                }
                match default {
                    Some(d) => {
                        let o = self.walk(d, input);
                        merge(&mut out.fall, &o.fall);
                        merge(&mut out.brk, &o.brk);
                        merge(&mut out.cont, &o.cont);
                    }
                    None => merge(&mut out.fall, &input),
                }
                out
            }
            LoopStmt | DoWhileStmt => {
                let Some(body) = loop_body(c, s) else { return out };
                if loop_kw(c, s) == "forever" {
                    let o = self.walk(body, input);
                    out.fall = o.brk;
                    return out;
                }
                let zero = c.kind(s) != DoWhileStmt;
                let o = self.walk(body, input.clone());
                if zero {
                    merge(&mut out.fall, &input);
                }
                merge(&mut out.fall, &o.fall);
                merge(&mut out.fall, &o.cont);
                merge(&mut out.fall, &o.brk);
                out
            }
            TimedStmt | WaitStmt => {
                match kids(c, s).into_iter().filter(|&x| is_stmt(c.kind(x))).last() {
                    Some(x) => self.walk(x, input),
                    None => {
                        out.fall = input;
                        out
                    }
                }
            }
            ReturnStmt => {
                for st in &input {
                    self.h.returned(s, st);
                }
                out
            }
            JumpStmt => {
                if c.keyword(s).is_some_and(|k| c.text_of(k) == "break") {
                    out.brk = input;
                } else {
                    out.cont = input;
                }
                out
            }
            _ => {
                if stops(c, s) {
                    return out;
                }
                let mut v = input;
                for st in v.iter_mut() {
                    self.h.leaf(s, st);
                }
                merge(&mut out.fall, &v);
                out
            }
        }
    }
}

/// A case statement's arm statements and its default's.
pub(crate) fn case_arms(c: &Cst, s: NodeId) -> (Vec<NodeId>, Option<NodeId>) {
    let mut arms = Vec::new();
    let mut default = None;
    let mut after_default = false;
    let mut seen_colon = false;
    for k in c.kids(s) {
        match *k {
            Child::Token(t) => {
                let tx = c.text_of(t);
                if tx == "default" {
                    after_default = true;
                } else if tx == ":" {
                    seen_colon = true;
                }
            }
            Child::Node(n) => {
                if is_stmt(c.kind(n)) && seen_colon {
                    if after_default {
                        default = Some(n);
                        after_default = false;
                    } else {
                        arms.push(n);
                    }
                    seen_colon = false;
                }
            }
        }
    }
    (arms, default)
}

fn lock_not_released(c: &Cst, s: &mut Sink) {
    for (r, n) in user_routines(c) {
        let mut hooks = LockHooks { c, released: BTreeSet::new(), acquired_at: HashMap::new(), leaks: Vec::new(), reported: BTreeSet::new() };
        let Some(body) = c.tree.child(r, Body) else { continue };
        for call in calls_in(c, body) {
            let (op, key) = classify(c, call);
            if op == LockOp::Release {
                hooks.released.insert(key);
            }
        }
        if hooks.released.is_empty() {
            continue;
        }
        let fall = {
            let mut w = Walker { h: &mut hooks };
            let it = items(c, body);
            w.list(&it, vec![Held::new()]).fall
        };
        let name_loc = c.tree.child(r, Name).and_then(|nm| {
            let toks = c.tree.token_range(nm);
            toks.last().map(|&t| c.loc(t))
        });
        for st in &fall {
            hooks.leak(st, None);
        }
        let rname = routine_name(c, r);
        for (key, at) in hooks.leaks.clone() {
            let acq = hooks.acquired_at.get(&key).map_or(0, |l| match l.expansion {
                Some((_, line)) => line,
                None => l.line,
            });
            let how = match at {
                None => format!("some path through '{rname}' ends without releasing it"),
                Some(_) => format!("this return leaves '{rname}' while still holding it"),
            };
            let Some(loc) = at.or(name_loc) else { continue };
            add_n(
                s,
                n,
                "SV-RSC-001",
                "LockNotReleased",
                Severity::Error,
                format!("'{key}' is acquired at line {acq} but {how}. The next requester then waits forever. Release it before every exit."),
                loc,
            );
        }
    }
}

// ---- SV-RSC-002 ----

fn file_not_closed(c: &Cst, s: &mut Sink) {
    for cls in user_classes(c) {
        let mut opens = Vec::new();
        let mut closes = false;
        for m in class_methods(c, cls) {
            let Some(body) = c.tree.child(m, Body) else { continue };
            for call in calls_in(c, body) {
                let n = call_parts(c, call).map(|p| p.1).unwrap_or_default();
                if n == "$fopen" {
                    opens.push(call);
                } else if n == "$fclose" {
                    closes = true;
                }
            }
        }
        if closes {
            continue;
        }
        let cname = decl_name(c, cls);
        for call in opens {
            let Some(loc) = start(c, call) else { continue };
            s.add(
                "SV-RSC-002",
                "FileHandleNotClosed",
                Severity::Warning,
                format!(
                    "'{cname}' opens a file with {} but never calls $fclose. Close it in final_phase or report_phase so buffered output is flushed and the handle is released.",
                    norm_text(c, call)
                ),
                loc,
            );
        }
    }
}

// ---- SV-THR-001 ----

fn fork_loop_variable(c: &Cst, s: &mut Sink) {
    for l in c.nodes(LoopStmt) {
        if loop_kw(c, l) != "for" {
            continue;
        }
        let Some(body) = loop_body(c, l) else { continue };
        // Loop variables: declared in the initializer or assigned there.
        let mut vars: HashSet<u32> = HashSet::new();
        let mut names: HashSet<String> = HashSet::new();
        let mut semis = 0;
        for k in c.kids(l) {
            match *k {
                Child::Token(t) if c.text_of(t) == ";" => semis += 1,
                Child::Node(x) if semis == 0 => {
                    if c.kind(x) == DataDecl {
                        for d in c.tree.children(x).filter(|&d| c.kind(d) == Declarator) {
                            vars.insert(d.0);
                        }
                    } else if let Some(lhs) = assign_lhs(c, x) {
                        if c.kind(lhs) == NameExpr {
                            match name_sym(c, lhs) {
                                Some((Sym::Decl(d), _, _)) => {
                                    vars.insert(d);
                                }
                                Some((_, nm, _)) => {
                                    names.insert(nm);
                                }
                                None => {}
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if vars.is_empty() && names.is_empty() {
            continue;
        }
        let n = times(c, l);
        for f in subtree(c, body) {
            if c.kind(f) != ParBlock || !matches!(join_kind(c, f), "join_none" | "join_any") {
                continue;
            }
            // slang visits only the expressions of a one-statement body.
            let it = items(c, f);
            if it.len() != 1 {
                continue;
            }
            let st = it[0];
            for e in stmt_exprs(c, st) {
                for x in subtree(c, e) {
                    if c.kind(x) != NameExpr {
                        continue;
                    }
                    let hit = match name_sym(c, x) {
                        Some((Sym::Decl(d), _, _)) => vars.contains(&d),
                        Some((Sym::Name(nm), _, _)) => names.contains(&nm),
                        _ => false,
                    };
                    if !hit {
                        continue;
                    }
                    let Some(t) = c.first_tok(x) else { continue };
                    let name = c.text_of(t);
                    add_n(
                        s,
                        n,
                        "SV-THR-001",
                        "ForkLoopVariableShared",
                        Severity::Error,
                        format!(
                            "Loop variable '{name}' is captured by reference inside a fork..join_none/any block. \
                             Copy to an automatic local variable before the fork to prevent race conditions."
                        ),
                        c.loc(t),
                    );
                }
            }
        }
    }
}

/// The expressions a statement holds itself (not those of the statements
/// inside it): what slang's `visitExprs` reaches.
fn stmt_exprs(c: &Cst, st: NodeId) -> Vec<NodeId> {
    match c.kind(st) {
        ExprStmt => kids(c, st).into_iter().filter(|&x| !matches!(c.kind(x), StmtLabel | Attribute)).collect(),
        IfStmt => if_parts(c, st).0.into_iter().collect(),
        ReturnStmt | EventTrigger | DisableStmt => kids(c, st),
        LoopStmt | DoWhileStmt => kids(c, st).into_iter().filter(|&x| !is_stmt(c.kind(x)) && !matches!(c.kind(x), StmtLabel | Attribute)).collect(),
        WaitStmt | CaseStmt => kids(c, st).into_iter().filter(|&x| !is_stmt(c.kind(x)) && !matches!(c.kind(x), StmtLabel | Attribute)).collect(),
        TimedStmt => timing_of(c, st).into_iter().collect(),
        _ => Vec::new(),
    }
}

// ---- SV-THR-002 ----

fn static_routine_variable(c: &Cst, s: &mut Sink) {
    for k in [FunctionDecl, TaskDecl] {
        for r in c.nodes(k) {
            if enclosing_class(c, r).is_some() {
                continue;
            }
            if has_kw(c, r, "automatic") {
                continue;
            }
            // The routine's default lifetime is its unit's.
            let unit = ancestors(c, r).find(|&a| matches!(c.kind(a), ModuleDecl | InterfaceDecl | ProgramDecl | PackageDecl | CheckerDecl));
            if unit.is_some_and(|u| has_kw(c, u, "automatic")) {
                continue;
            }
            let Some(body) = c.tree.child(r, Body) else { continue };
            let n = times(c, r);
            let rname = routine_name(c, r);
            for d in kids(c, body) {
                if c.kind(d) != DataDecl {
                    continue;
                }
                let toks = tok_texts(c, d);
                if toks.iter().any(|t| matches!(*t, "automatic" | "const" | "typedef" | "input" | "output" | "inout" | "ref")) {
                    continue;
                }
                if c.tree.child(d, DataType).is_some_and(|t| tok_texts(c, t).first() == Some(&"const")) {
                    continue;
                }
                for decl in c.tree.children(d).filter(|&x| c.kind(x) == Declarator) {
                    let Some(nm) = c.tree.child(decl, Name) else { continue };
                    let Some(t) = c.first_tok(nm) else { continue };
                    let vname = c.text_of(t);
                    add_n(
                        s,
                        n,
                        "SV-THR-002",
                        "StaticRoutineVariableLifetime",
                        Severity::Error,
                        format!(
                            "Local variable '{vname}' in routine '{rname}' has implicit static lifetime. In non-class scopes, \
                             tasks and functions default to static lifetime, causing local variables to be shared \
                             across concurrent invocations and inducing race conditions. Declare routine as 'automatic'."
                        ),
                        c.loc(t),
                    );
                }
            }
        }
    }
}

// ---- SV-THR-003 ----

fn unbounded_spawn(c: &Cst, s: &mut Sink) {
    for (r, n) in user_routines(c) {
        let where_ = format!("'{}'", routine_name(c, r));
        for l in subtree(c, r) {
            if !is_wait_loop(c, l) {
                continue;
            }
            let kind = if c.kind(l) == DoWhileStmt { "do-while" } else { loop_kw(c, l) };
            let Some(body) = loop_body(c, l) else { continue };
            let mut spawn = None;
            let mut bounded = false;
            let mut stack = vec![body];
            while let Some(x) = stack.pop() {
                match c.kind(x) {
                    ParBlock if join_kind(c, x) == "join_none" => {
                        if spawn.is_none() {
                            spawn = Some(x);
                        }
                        continue;
                    }
                    WaitStmt if c.toks(x).any(|t| c.text_of(t) == "fork") => bounded = true,
                    DisableStmt if c.toks(x).nth(1).is_some_and(|t| c.text_of(t) == "fork") => bounded = true,
                    CallExpr if is_semaphore_get(c, x) => bounded = true,
                    _ => {}
                }
                let k: Vec<NodeId> = c.tree.children(x).collect();
                stack.extend(k.into_iter().rev());
            }
            let Some(f) = spawn else { continue };
            if bounded {
                continue;
            }
            let Some(loc) = start(c, f) else { continue };
            add_n(
                s,
                n,
                "SV-THR-003",
                "UnboundedThreadSpawn",
                Severity::Warning,
                format!(
                    "fork ... join_none inside a {kind} loop in {where_} starts a new thread every iteration with nothing that limits how many run at once. \
                     Threads pile up when the consumer is slow and are still running at end of test; bound \
                     them with a semaphore, or wait fork before the next batch."
                ),
                loc,
            );
        }
    }
}
