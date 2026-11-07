//! Group E: what the AST-walking C++ rules of the group need from slang's
//! semantic model, read off the native syntax tree and sv-sema's checks.
//!
//! - [`ast_ok`]: slang's AST visitors (`VisitFlags::AllGood`) skip an
//!   expression that failed to bind, and so all of the statement-level
//!   expression around it. Here an expression is bad when it overlaps an
//!   sv-sema error (undeclared names, unknown types) or names a variable
//!   whose declared type is one.
//!
//! Each tree is checked on its own (repo tests, sv-tests and uvm_pkg are
//! each one tree); a name another tree declares then reads as undeclared,
//! which only drops findings.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use sv_syntax::{NodeId, SyntaxKind, Tree};

use crate::g_random::{kids, subtree};
use crate::syntax::Cst;
use sv_syntax::SyntaxKind::*;

pub(crate) struct Info {
    /// Token ranges of nodes sv-sema reports an error at.
    bad: Vec<(u32, u32)>,
    /// Token ranges of expressions slang rejects without rejecting the
    /// statement around them.
    bad_expr: Vec<(u32, u32)>,
    /// Variables whose declared type does not resolve.
    poisoned: HashSet<String>,
    /// Every name the tree declares (any scope), for names sv-sema does
    /// not check (scopes importing an unknown package, classes extending
    /// an unknown base): slang binds those as errors without a message.
    declared: HashSet<String>,
    /// Objects whose class extends an unresolved base.
    tainted: HashSet<String>,
    /// Classes extending an unresolved base.
    pub(crate) tainted_classes: HashSet<String>,
}

thread_local! {
    static CACHE: RefCell<Vec<(usize, usize, Rc<Info>)>> = const { RefCell::new(Vec::new()) };
}

/// What the whole compile says: sv-sema's diagnostics by tree (the
/// nodes), and the names every other tree declares at its top.
struct Whole {
    diags: std::collections::HashMap<usize, Vec<sv_syntax::NodeId>>,
    declared: HashSet<String>,
}

thread_local! {
    static WHOLE: RefCell<Option<((usize, usize), Rc<Whole>)>> = const { RefCell::new(None) };
}

fn whole(c: &Cst) -> Rc<Whole> {
    let all = c.all;
    let key = (all.as_ptr() as usize, all.len());
    if let Some(w) = WHOLE.with(|k| k.borrow().as_ref().filter(|e| e.0 == key).map(|e| e.1.clone())) {
        return w;
    }
    let scopes = sv_sema::scopes::build(all);
    let mut diags = sv_sema::names::check(all, &scopes);
    diags.extend(sv_sema::bind::check(all));
    diags.extend(sv_sema::checks::check(all, &scopes));
    diags.extend(sv_sema::order::check(all, &scopes));
    diags.extend(sv_sema::calls::check(all));
    diags.extend(sv_sema::typed::check(all, &scopes));
    let mut by_tree: std::collections::HashMap<usize, Vec<sv_syntax::NodeId>> = Default::default();
    for d in diags {
        // Constraint mode errors leave the expression itself bound.
        if d.message.starts_with("randc variable") {
            continue;
        }
        by_tree.entry(d.tree).or_default().push(d.node);
    }
    let mut declared = HashSet::new();
    if all.len() > 1 {
        for t in all {
            for p in t.preorder() {
                if t.kind(p) == DataType {
                    continue;
                }
                for n in t.children(p).filter(|&n| t.kind(n) == Name) {
                    declared.insert(t.bare_text(n));
                }
            }
        }
    }
    let w = Rc::new(Whole { diags: by_tree, declared });
    WHOLE.with(|k| *k.borrow_mut() = Some((key, w.clone())));
    w
}

pub(crate) fn info(c: &Cst) -> Rc<Info> {
    let key = (c.tree as *const Tree as usize, c.tree.tokens.len());
    if let Some(i) = CACHE.with(|k| k.borrow().iter().find(|e| (e.0, e.1) == key).map(|e| e.2.clone())) {
        return i;
    }
    let all = whole(c);
    let diags = all.diags.get(&c.index()).map(|v| &v[..]).unwrap_or(&[]);
    let mut bad = Vec::new();
    let mut poisoned = HashSet::new();
    for &node in diags {
        let Some(range) = c.tree.token_bounds(node) else { continue };
        // `ShadowItem x;` with ShadowItem unknown: x has an error type,
        // which makes the expressions that use x bad, not the declaration.
        let mut n = node;
        let mut typed_decl = false;
        while let Some(p) = c.parent(n) {
            if matches!(c.kind(p), DataDecl | Port) && c.kind(n) == DataType {
                for dcl in c.tree.children(p).filter(|&x| c.kind(x) == Declarator) {
                    if let Some(nm) = c.tree.child(dcl, Name) {
                        poisoned.insert(c.tree.bare_text(nm));
                    }
                }
                typed_decl = true;
                break;
            }
            if is_stmt(c.kind(p)) {
                break;
            }
            n = p;
        }
        if !typed_decl {
            bad.push(range);
        }
    }
    let mut bad_expr = Vec::new();
    // `x = new(a, b)` with more arguments than x's class constructor takes.
    for nx in c.nodes(NewExpr) {
        let Some(al) = c.tree.child(nx, ArgList) else { continue };
        let args: Vec<NodeId> = c.tree.children(al).filter(|&a| c.kind(a) == Arg).collect();
        if args.is_empty() || args.iter().any(|&a| c.first_tok(a).is_some_and(|t| c.text_of(t) == ".")) {
            continue;
        }
        let Some(class) = new_target_class(c, nx) else { continue };
        let Some(formals) = ctor_formals(c, &class) else { continue };
        if args.len() > formals {
            if let Some(r) = c.tree.token_bounds(nx) {
                bad_expr.push(r);
            }
        }
    }
    // `f = '{1, 2, 3}` for `int f[4]`, `b[0:3]` for `logic [7:0] b`.
    for n in c.tree.preorder() {
        let wrong = match c.kind(n) {
            AssignPattern => pattern_count_mismatch(c, n),
            SelectExpr => reversed_select(c, n),
            _ => false,
        };
        if wrong {
            if let Some(r) = c.tree.token_bounds(n) {
                bad_expr.push(r);
            }
        }
    }
    // A delay or wait in a function (outside fork...join_none) does not bind.
    for n in c.tree.preorder() {
        if !matches!(c.kind(n), TimedStmt | WaitStmt) {
            continue;
        }
        let mut in_function = false;
        for a in std::iter::successors(c.parent(n), |&a| c.parent(a)) {
            match c.kind(a) {
                ParBlock if c.tok(a, "join_none").is_some() => break,
                FunctionDecl => {
                    in_function = true;
                    break;
                }
                TaskDecl | ProceduralBlock | ClassDecl | ModuleDecl => break,
                _ => {}
            }
        }
        if in_function {
            if let Some(r) = c.tree.token_bounds(n) {
                bad.push(r);
            }
        }
    }
    let mut declared: HashSet<String> = all.declared.clone();
    for n in c.tree.preorder() {
        match c.kind(n) {
            Name if c.parent(n).is_some_and(|p| c.kind(p) != DataType) => {
                declared.insert(c.tree.bare_text(n));
                if let Some((_, b)) = c.tree.token_bounds(n) {
                    declared.insert(c.text_of(b).to_string());
                }
            }
            LoopStmt | ConstraintItem => {
                for t in c.toks(n) {
                    declared.insert(c.text_of(t).to_string());
                }
            }
            _ => {}
        }
    }
    // Pattern variables: `o matches tagged Some .v`, `case (o) matches`.
    let toks = &c.tree.tokens;
    if toks.iter().any(|t| &*t.text == "matches") {
        for w in toks.windows(2) {
            if &*w[0].text == "." && w[1].kind == sv_syntax::TokenKind::Identifier {
                declared.insert(w[1].text.to_string());
            }
        }
    }
    // Locals of an unknown type: `uvm_phase p;` with uvm_pkg missing.
    for n in c.tree.preorder() {
        if !matches!(c.kind(n), DataDecl | Port) {
            continue;
        }
        let Some(dt) = c.tree.child(n, DataType) else { continue };
        let Some(first) = c.first_tok(dt) else { continue };
        let tok = &c.tree.tokens[first as usize];
        if tok.kind != sv_syntax::TokenKind::Identifier || declared.contains(&*tok.text) || is_builtin(&tok.text) {
            continue;
        }
        for dcl in c.tree.children(n).filter(|&x| c.kind(x) == Declarator) {
            if let Some(nm) = c.tree.child(dcl, Name) {
                poisoned.insert(c.tree.bare_text(nm));
            }
        }
    }
    // Objects of a class whose base does not resolve: slang binds member
    // accesses through them as errors without a message.
    let tainted_classes = tainted_classes(c, &declared);
    let mut tainted = HashSet::new();
    if !tainted_classes.is_empty() {
        for n in c.tree.preorder() {
            if !matches!(c.kind(n), DataDecl | Port) {
                continue;
            }
            let Some(dt) = c.tree.child(n, DataType) else { continue };
            let ty = c.tree.bare_text(dt);
            let hit = tainted_classes.contains(&ty) || {
                // `T rec` with `type T = tainted` on the enclosing class.
                std::iter::successors(c.parent(n), |&a| c.parent(a)).filter(|&a| c.kind(a) == ClassDecl).any(|cls| {
                    c.tree.child(cls, Header).and_then(|h| c.tree.child(h, ParamList)).is_some_and(|pl| {
                        c.tree.children(pl).any(|pd| {
                            c.tree.child(pd, Name).is_some_and(|nm| c.tree.bare_text(nm) == ty)
                                && c.tree.child(pd, DataType).is_some_and(|d| tainted_classes.contains(&c.tree.bare_text(d)))
                        })
                    })
                })
            };
            if !hit {
                continue;
            }
            for dcl in c.tree.children(n).filter(|&x| c.kind(x) == Declarator) {
                if let Some(nm) = c.tree.child(dcl, Name) {
                    tainted.insert(c.tree.bare_text(nm));
                }
            }
        }
    }
    let i = Rc::new(Info { bad, bad_expr, poisoned, declared, tainted, tainted_classes });
    CACHE.with(|k| {
        let mut k = k.borrow_mut();
        if k.len() > 8 {
            k.remove(0);
        }
        k.push((key.0, key.1, i.clone()));
    });
    i
}

pub(crate) fn is_expr(k: SyntaxKind) -> bool {
    matches!(
        k,
        BinaryExpr
            | PrefixExpr
            | PostfixExpr
            | CondExpr
            | InsideExpr
            | RangeList
            | OpenRange
            | ParenExpr
            | Literal
            | NameExpr
            | ScopedExpr
            | MemberExpr
            | SelectExpr
            | CallExpr
            | ArgList
            | Arg
            | WithClause
            | ConcatExpr
            | AssignPattern
            | CastExpr
            | NewExpr
            | TaggedExpr
            | MatchExpr
            | TimedExpr
            | TypeExpr
            | ExprOrDist
            | DistItem
    )
}

fn is_stmt(k: SyntaxKind) -> bool {
    matches!(
        k,
        ExprStmt
            | IfStmt
            | CaseStmt
            | LoopStmt
            | DoWhileStmt
            | TimedStmt
            | WaitStmt
            | AssertStmt
            | ReturnStmt
            | SeqBlock
            | ParBlock
            | Body
            | ConstraintItem
            | ConstraintBlock
            | ProceduralBlock
            | ContinuousAssign
            | DataDecl
    )
}

const BUILTINS: &[&str] = &[
    "this", "super", "null", "std", "local", "process", "semaphore", "mailbox", "weak_reference",
    "option", "type_option", "item", "index", "randomize", "new", "CrossQueueType", "srandom", "get_randstate",
    "set_randstate", "rand_mode", "constraint_mode",
];

fn is_builtin(t: &str) -> bool {
    BUILTINS.contains(&t) || t.starts_with('$')
}

/// What slang binds as one unit, all good or all skipped: a subroutine's
/// body, a procedural block, a constraint block (a bad statement makes its
/// whole list bad), else the outermost expression.
pub(crate) fn bind_root(c: &Cst, n: NodeId) -> NodeId {
    let mut x = n;
    while let Some(p) = c.parent(x) {
        match c.kind(p) {
            Body if c.parent(p).is_some_and(|f| matches!(c.kind(f), FunctionDecl | TaskDecl)) => return p,
            ProceduralBlock | ConstraintDecl => return p,
            _ => {}
        }
        x = p;
    }
    top_expr(c, n)
}

/// The outermost expression around `n` (slang binds and checks it as one).
pub(crate) fn top_expr(c: &Cst, mut n: NodeId) -> NodeId {
    while let Some(p) = c.parent(n) {
        if !is_expr(c.kind(p)) {
            break;
        }
        n = p;
    }
    n
}

/// The one declaration of `name` in the tree: its type and declarator.
fn decl_of(c: &Cst, name: &str) -> Option<(NodeId, NodeId)> {
    let mut found = None;
    for dd in c.nodes(DataDecl) {
        for dcl in c.tree.children(dd).filter(|&x| c.kind(x) == Declarator) {
            if c.tree.child(dcl, Name).is_some_and(|nm| c.tree.bare_text(nm) == name) {
                if found.is_some() {
                    return None;
                }
                found = Some((c.tree.child(dd, DataType)?, dcl));
            }
        }
    }
    found
}

fn small_int(c: &Cst, n: NodeId) -> Option<i64> {
    if c.kind(n) != Literal {
        return None;
    }
    let t = c.first_tok(n)?;
    let text = c.text_of(t);
    text.bytes().all(|b| b.is_ascii_digit()).then(|| text.parse().ok()).flatten()
}

/// The variable an expression is assigned to or initializes.
fn assign_target(c: &Cst, e: NodeId) -> Option<(NodeId, NodeId)> {
    let p = c.parent(e)?;
    match c.kind(p) {
        BinaryExpr if c.tok(p, "=").is_some() && c.tree.children(p).nth(1) == Some(e) => {
            let l = c.tree.children(p).next()?;
            if c.kind(l) != NameExpr {
                return None;
            }
            decl_of(c, &c.tree.bare_text(l))
        }
        Declarator => Some((c.tree.child(c.parent(p)?, DataType)?, p)),
        _ => None,
    }
}

pub(crate) fn pattern_count_mismatch(c: &Cst, ap: NodeId) -> bool {
    if c.tok(ap, ":").is_some() || c.tree.child(ap, ConcatExpr).is_some() {
        return false;
    }
    let Some((dt, dcl)) = assign_target(c, ap) else { return false };
    let dims: Vec<NodeId> = c.tree.children(dcl).filter(|&d| c.kind(d) == Dim).collect();
    let [d] = dims[..] else { return false };
    if c.tok(d, ":").is_some() || c.tree.child(dt, Dim).is_some() || c.tree.children(d).count() != 1 {
        return false;
    }
    let Some(size) = c.tree.children(d).next().and_then(|x| small_int(c, x)) else { return false };
    let given = c.tree.children(ap).count() as i64;
    given != size
}

fn reversed_select(c: &Cst, s: NodeId) -> bool {
    if c.tok(s, ":").is_none() || c.tok(s, "+:").is_some() || c.tok(s, "-:").is_some() {
        return false;
    }
    let k: Vec<NodeId> = c.tree.children(s).collect();
    let [base, l, r] = k[..] else { return false };
    if c.kind(base) != NameExpr {
        return false;
    }
    let (Some(l), Some(r)) = (small_int(c, l), small_int(c, r)) else { return false };
    let Some((dt, dcl)) = decl_of(c, &c.tree.bare_text(base)) else { return false };
    if c.tree.child(dcl, Dim).is_some() {
        return false;
    }
    let dims: Vec<NodeId> = c.tree.children(dt).filter(|&d| c.kind(d) == Dim).collect();
    let [d] = dims[..] else { return false };
    let b: Vec<NodeId> = c.tree.children(d).collect();
    let [hi, lo] = b[..] else { return false };
    let (Some(hi), Some(lo)) = (small_int(c, hi), small_int(c, lo)) else { return false };
    l != r && hi != lo && (hi > lo) != (l > r)
}

/// The declared class of the variable `new` initializes or is assigned
/// to, by name.
fn new_target_class(c: &Cst, nx: NodeId) -> Option<String> {
    let p = c.parent(nx)?;
    let var = match c.kind(p) {
        BinaryExpr if c.tok(p, "=").is_some() && c.tree.children(p).nth(1) == Some(nx) => {
            let l = c.tree.children(p).next()?;
            (c.kind(l) == NameExpr).then(|| c.tree.bare_text(l))?
        }
        Declarator => {
            let dd = c.parent(p)?;
            return c.tree.child(dd, DataType).map(|d| c.tree.bare_text(d));
        }
        _ => return None,
    };
    // The nearest declaration of the name: a local or a class member.
    let mut found = None;
    for dd in c.nodes(DataDecl) {
        for dcl in c.tree.children(dd).filter(|&x| c.kind(x) == Declarator) {
            if c.tree.child(dcl, Name).is_some_and(|nm| c.tree.bare_text(nm) == var) {
                let ty = c.tree.child(dd, DataType).map(|d| c.tree.bare_text(d))?;
                if found.as_ref().is_some_and(|f| f != &ty) {
                    return None;
                }
                found = Some(ty);
            }
        }
    }
    found
}

/// How many formals the constructor of the one class of this name in the
/// tree declares, when it declares one.
fn ctor_formals(c: &Cst, class: &str) -> Option<usize> {
    let mut classes = c.nodes(ClassDecl).into_iter().filter(|&k| c.tree.child(k, Name).is_some_and(|n| c.tree.bare_text(n) == class));
    let k = classes.next()?;
    if classes.next().is_some() {
        return None;
    }
    let f = c.tree.children(k).find(|&f| {
        c.kind(f) == FunctionDecl && c.tree.child(f, Name).is_some_and(|n| c.tree.bare_text(n) == "new")
    })?;
    let pl = c.tree.child(f, PortList)?;
    Some(c.tree.children(pl).filter(|&p| c.kind(p) == Port).count())
}

/// Classes extending a base that does not resolve, directly or through
/// another such class.
fn tainted_classes(c: &Cst, declared: &HashSet<String>) -> HashSet<String> {
    let mut base: Vec<(String, String)> = Vec::new();
    for cls in c.nodes(ClassDecl) {
        let Some(nm) = c.tree.child(cls, Name) else { continue };
        let ext = c.tree.child(cls, Header).into_iter().flat_map(|h| c.tree.children(h)).find(|&e| {
            c.kind(e) == Extends && c.first_tok(e).is_some_and(|t| c.text_of(t) == "extends")
        });
        let Some(dt) = ext.and_then(|e| c.tree.child(e, DataType)) else { continue };
        let Some(first) = c.first_tok(dt) else { continue };
        base.push((c.tree.bare_text(nm), c.text_of(first).to_string()));
    }
    let mut out: HashSet<String> =
        base.iter().filter(|(_, b)| !declared.contains(b) && !is_builtin(b)).map(|(n, _)| n.clone()).collect();
    loop {
        let before = out.len();
        for (n, b) in &base {
            if out.contains(b) {
                out.insert(n.clone());
            }
        }
        if out.len() == before {
            return out;
        }
    }
}

/// slang's AST has this expression (it bound without error).
pub(crate) fn ast_ok(c: &Cst, n: NodeId) -> bool {
    let i = info(c);
    if !i.bad_expr.is_empty() {
        if let Some((a, b)) = c.tree.token_bounds(top_expr(c, n)) {
            if i.bad_expr.iter().any(|&(x, y)| a <= x && y <= b) {
                return false;
            }
        }
    }
    expr_ok(c, &i, bind_root(c, n))
}

pub(crate) fn expr_ok(c: &Cst, i: &Info, top: NodeId) -> bool {
    let Some((a, b)) = c.tree.token_bounds(top) else { return true };
    if i.bad.iter().any(|&(x, y)| a <= x && y <= b) {
        return false;
    }
    !subtree(c, top).into_iter().any(|x| {
        if c.kind(x) == MemberExpr && !i.tainted.is_empty() {
            let mut b = x;
            while let Some(k) = c.tree.children(b).next().filter(|_| matches!(c.kind(b), MemberExpr | SelectExpr)) {
                b = k;
            }
            if c.kind(b) == NameExpr && i.tainted.contains(&c.tree.bare_text(b)) {
                return true;
            }
        }
        if c.kind(x) != NameExpr {
            return false;
        }
        let Some(&t) = c.toks(x).collect::<Vec<_>>().first() else { return false };
        let tok = &c.tree.tokens[t as usize];
        let text = &*tok.text;
        // `find(x) with (x > 1)`, `map(e) with (e * 2)`: iterator names.
        let iterator = || {
            std::iter::successors(c.parent(x), |&a| c.parent(a))
                .take_while(|&a| a != top)
                .any(|a| c.kind(a) == CallExpr && c.tree.child(a, WithClause).is_some())
        };
        if !i.poisoned.contains(text) && iterator() {
            return false;
        }
        i.poisoned.contains(text)
            || (tok.kind == sv_syntax::TokenKind::Identifier && !i.declared.contains(text) && !is_builtin(text))
    })
}

#[allow(dead_code)]
pub(crate) fn first_kid(c: &Cst, n: NodeId) -> Option<NodeId> {
    kids(c, n).into_iter().next()
}

/// How often slang's AST visitor reaches this node: once, or once per
/// specialization of the generic class around it (see [`spec_counts`]).
pub(crate) fn visits(_c: &Cst, _n: NodeId) -> usize {
    1
}
