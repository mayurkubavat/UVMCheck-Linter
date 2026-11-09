//! Statements, loops, timing controls, calls and the variables they write, as
//! the concurrency rules read them. Names resolve through
//! [`crate::support::slang_ast`].

use dv_model::Loc;

use sv_syntax::{Child, NodeId, SyntaxKind};

use crate::support::slang_ast::*;

use crate::syntax::Cst;

use crate::{Severity, Sink};

use SyntaxKind::*;

/// Adds the same finding `n` times (once per visit of slang's AST walk).
pub(crate) fn add_n(
    s: &mut Sink,
    n: usize,
    rule: &'static str,
    name: &'static str,
    sev: Severity,
    msg: String,
    loc: Loc,
) {
    for _ in 0..n {
        s.add(rule, name, sev, msg.clone(), loc);
    }
}

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

pub(crate) fn loop_kw<'a>(c: &'a Cst, l: NodeId) -> &'a str {
    if c.kind(l) == DoWhileStmt {
        return "do";
    }
    c.toks(l)
        .map(|t| c.text_of(t))
        .find(|t| matches!(*t, "for" | "while" | "repeat" | "forever" | "foreach"))
        .unwrap_or("")
}

/// `forever`, `while` and `do ... while` loops (slang's
/// ForeverLoop/WhileLoop/DoWhileLoop statements).
pub(crate) fn is_wait_loop(c: &Cst, n: NodeId) -> bool {
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
pub(crate) fn routine_name(c: &Cst, r: NodeId) -> String {
    last_name(c, r)
}

/// The kind of token a timing control starts with: `#`, `##` or `@`.
pub(crate) fn timing_kind<'a>(c: &'a Cst, t: NodeId) -> &'a str {
    c.keyword(t).map_or("", |k| c.text_of(k))
}

/// A timed statement's timing control.
pub(crate) fn timing_of(c: &Cst, n: NodeId) -> Option<NodeId> {
    c.tree.child(n, TimingControl)
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
                Some(t) => BUILTIN_TYPES.contains(&t.as_str()) || c.named(TypedefDecl).contains_key(&t),
                None => !matches!(name.as_str(), "randomize"),
            }
        }
        None => false,
    }
}

pub(crate) const BUILTIN_TYPES: &[&str] = &[
    "bit",
    "logic",
    "reg",
    "int",
    "integer",
    "byte",
    "shortint",
    "longint",
    "string",
    "real",
    "shortreal",
    "time",
    "realtime",
    "event",
    "chandle",
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

pub(crate) const ASSIGN_OPS: &[&str] =
    &["=", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=", "<<<=", ">>>="];

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
pub(crate) fn formal_dirs(c: &Cst, decl: NodeId) -> Vec<&'static str> {
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

pub(crate) const MUTATING: &[&str] =
    &["$cast", "$sscanf", "$fscanf", "$fgets", "$fread", "$value$plusargs", "randomize", "srandom"];

pub(crate) const CONTAINER: &[&str] =
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

/// Calls the C++ rules take for a blocking task call: a user task, or one
/// this lookup cannot resolve (slang would know it).
pub(crate) fn is_task_call(c: &Cst, call: NodeId) -> bool {
    if is_system_call(c, call) {
        return false;
    }
    match callee_decl(c, call) {
        Some(d) => c.kind(d) == TaskDecl || (c.kind(d) == MethodPrototype && has_kw(c, d, "task")),
        None => !builtin_function(c, call),
    }
}

/// Methods of built-in classes and types that are functions.
pub(crate) fn builtin_function(c: &Cst, call: NodeId) -> bool {
    let Some((_, name, recv)) = call_parts(c, call) else { return false };
    let ty = recv.and_then(|r| expr_type(c, r));
    match ty.as_deref() {
        Some("semaphore") => matches!(name.as_str(), "try_get" | "put" | "new"),
        Some("mailbox") => matches!(name.as_str(), "try_get" | "try_put" | "try_peek" | "num" | "new"),
        Some("process") => !matches!(name.as_str(), "await"),
        _ => matches!(
            name.as_str(),
            "randomize" | "new" | "srandom" | "get_randstate" | "set_randstate" | "rand_mode" | "constraint_mode"
        ),
    }
}

/// Methods of a class declared in it (an out-of-block body counts).
pub(crate) fn own_methods(c: &Cst, cls: NodeId) -> Vec<NodeId> {
    class_methods(c, cls)
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
    pub(crate) brk: Vec<S>,
    pub(crate) cont: Vec<S>,
}

impl<S> Default for Out<S> {
    fn default() -> Self {
        Out { fall: Vec::new(), brk: Vec::new(), cont: Vec::new() }
    }
}

pub(crate) const MAX_STATES: usize = 32;

pub(crate) fn merge<S: Clone + PartialEq>(into: &mut Vec<S>, from: &[S]) {
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
        if matches!(
            n.as_str(),
            "uvm_report_enabled" | "get_report_verbosity_level" | "get_report_action" | "uvm_get_report_object"
        ) {
            report = true;
        } else if !crate::support::class_index::is_report_id_part(&n) {
            return false;
        }
    }
    report
}

pub(crate) fn stops(c: &Cst, s: NodeId) -> bool {
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

    pub(crate) fn block(&mut self, b: NodeId, input: Vec<H::St>) -> Out<H::St> {
        let it = items(self.h.cst(), b);
        self.list(&it, input)
    }

    pub(crate) fn walk(&mut self, s: NodeId, input: Vec<H::St>) -> Out<H::St> {
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
            TimedStmt | WaitStmt => match kids(c, s).into_iter().filter(|&x| is_stmt(c.kind(x))).last() {
                Some(x) => self.walk(x, input),
                None => {
                    out.fall = input;
                    out
                }
            },
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
