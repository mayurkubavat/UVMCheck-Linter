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
    /// Variables whose declared type does not resolve.
    poisoned: HashSet<String>,
    /// Every name the tree declares (any scope), for names sv-sema does
    /// not check (scopes importing an unknown package, classes extending
    /// an unknown base): slang binds those as errors without a message.
    declared: HashSet<String>,
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
    let i = Rc::new(Info { bad, poisoned, declared });
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

/// slang's AST has this expression (it bound without error).
pub(crate) fn ast_ok(c: &Cst, n: NodeId) -> bool {
    let i = info(c);
    expr_ok(c, &i, bind_root(c, n))
}

pub(crate) fn expr_ok(c: &Cst, i: &Info, top: NodeId) -> bool {
    let Some((a, b)) = c.tree.token_bounds(top) else { return true };
    if i.bad.iter().any(|&(x, y)| a <= x && y <= b) {
        return false;
    }
    !subtree(c, top).into_iter().any(|x| {
        if c.kind(x) != NameExpr {
            return false;
        }
        let Some(&t) = c.toks(x).collect::<Vec<_>>().first() else { return false };
        let tok = &c.tree.tokens[t as usize];
        let text = &*tok.text;
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
