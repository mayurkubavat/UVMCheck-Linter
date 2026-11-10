//! Method bodies: statements, early exits, report guards, config_db calls and
//! who creates what (factory or new).

use std::collections::HashMap;

use dv_model::{Call, Class, Design, Loc, Method};

use svlang::lexer::Kind;

use svlang::syntax::{Child, NodeId};

use crate::support::class_roles::{class_methods, key, user_classes, Ctx, Index};

use crate::syntax::Cst;

use svlang::syntax::SyntaxKind::*;

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
        TimedStmt => c.items(n).into_iter().find(|&x| c.kind(x) != TimingControl).is_some_and(|s| always_exits(c, s)),
        _ => false,
    }
}

pub(crate) fn is_stop_call(c: &Cst, e: NodeId) -> bool {
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
        if matches!(
            name,
            "uvm_report_enabled" | "get_report_verbosity_level" | "get_report_action" | "uvm_get_report_object"
        ) {
            report = true;
        } else if !crate::support::class_index::is_report_id_part(name) {
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

pub(crate) fn is_active_early_exit(c: &Cst, s: NodeId) -> bool {
    if c.kind(s) != IfStmt {
        return false;
    }
    let (cond, then, other) = if_parts(c, s);
    other.is_none() && cond.is_some_and(|x| mentions_active(c, x)) && then.is_some_and(|t| always_exits(c, t))
}

pub(crate) type CallKey<'d> = ((u32, u32, u32, Option<(u32, u32)>), &'d str);

/// The model's calls of a method, by where they start and their name.
pub(crate) struct Calls<'d> {
    pub(crate) by: HashMap<CallKey<'d>, &'d Call>,
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

pub(crate) fn walk_creations<'d>(
    ctx: &Ctx<'d, '_>,
    calls: &Calls<'d>,
    c: &Cst,
    n: NodeId,
    guarded: bool,
    out: &mut Vec<Creation<'d>>,
) {
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
pub(crate) fn factory_create_type<'d>(
    ctx: &Ctx<'d, '_>,
    calls: &Calls<'d>,
    c: &Cst,
    call: NodeId,
) -> Option<&'d Class> {
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
pub(crate) fn scoped_new_type<'d>(ctx: &Ctx<'d, '_>, c: &Cst, call: NodeId) -> Option<&'d Class> {
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
pub(crate) fn bodies<'a, 'd, 't>(
    d: &'d Design,
    ix: &Index<'a, 't>,
) -> Vec<(&'d Class, &'d Method, &'a Cst<'t>, NodeId)> {
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

/// An integer literal's text (slang's IntegerLiteral: not a string, real,
/// time or unbased literal).
pub(crate) fn integer_literal(c: &Cst, e: NodeId) -> Option<String> {
    let e = c.strip(e);
    if c.kind(e) != Literal {
        return None;
    }
    let toks = c.tree.token_range(e);
    let ok = toks.iter().all(|&t| {
        matches!(c.tree.tokens[t as usize].kind, Kind::IntegerLiteral | Kind::IntegerBase | Kind::VectorDigits)
    });
    (ok && !toks.is_empty()).then(|| c.expr_text(e))
}
