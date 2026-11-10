//! Handle types, parameter overrides and the declarations expressions name.

use std::collections::HashSet;

use svlang::syntax::NodeId;

use svlang::syntax::SyntaxKind::*;

use crate::syntax::Cst;

use crate::support::slang_ast::{class_member, expr_type, find_class, kids, lookup, Decl};

/// What a type name names: a class (its declaration, when in the tree), or
/// another handle type (virtual interface, built-in class). Follows type
/// parameters (their default) and typedefs.
pub(crate) enum HandleTy {
    Class(NodeId),
    Other,
}

pub(crate) fn handle_type(c: &Cst, dtype: NodeId, ctx: Option<NodeId>, depth: u32) -> Option<HandleTy> {
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
        if d.kind != crate::support::slang_ast::DeclKind::Parameter || c.kind(d.node) != ParamDecl {
            return None;
        }
        if let Some((arg, x)) = ctx.and_then(|e| param_override(c, d.node, t, e)) {
            return handle_type(c, arg, Some(x), depth + 1);
        }
        // `type T = cls`: the default type.
        let toks = c.tree.token_range(d.node);
        let eq = *toks.iter().find(|&&x| c.text_of(x) == "=")?;
        let dt =
            kids(c, d.node).into_iter().find(|&k| c.kind(k) == DataType && c.first_tok(k).is_some_and(|f| f > eq))?;
        return handle_type(c, dt, ctx, depth + 1);
    }
    let td = c.first_named(TypedefDecl, t)?;
    let dt = c.tree.child(td, DataType)?;
    handle_type(c, dt, ctx, depth + 1)
}

/// The type a class below `ctx`'s chain passes for type parameter `name`
/// (declared at `param`) of its base, with the class that passes it.
pub(crate) fn param_override(c: &Cst, param: NodeId, name: &str, ctx: NodeId) -> Option<(NodeId, NodeId)> {
    let owner = crate::support::slang_ast::enclosing_class(c, param)?;
    let chain = crate::support::slang_ast::class_chain(c, ctx);
    let i = chain.iter().position(|&k| k == owner)?;
    let x = *chain.get(i.checked_sub(1)?)?;
    let ext = c.tree.child(c.tree.child(x, Header)?, Extends)?;
    let pl = c.tree.child(c.tree.child(ext, DataType)?, ParamList)?;
    let al = c.tree.child(pl, ArgList)?;
    let args: Vec<NodeId> = c.tree.children(al).filter(|&a| c.kind(a) == Arg).collect();
    let named = |a: NodeId| {
        let t: Vec<&str> = c.toks(a).map(|t| c.text_of(t)).collect();
        if t.first() == Some(&".") {
            t.get(1).copied()
        } else {
            None
        }
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
pub(crate) fn expr_class(c: &Cst, e: NodeId) -> Option<NodeId> {
    let d = expr_decl(c, e)?;
    match handle_type(c, d.dtype?, crate::support::slang_ast::enclosing_class(c, e), 0)? {
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
                NameExpr if c.tree.bare_text(base) == "this" => crate::support::slang_ast::enclosing_class(c, e)?,
                _ => expr_class(c, base).or_else(|| expr_type(c, base).and_then(|t| find_class(c, &t, e)))?,
            };
            class_member(c, cls, &member)
        }
        ParenExpr => expr_decl(c, *kids(c, e).first()?),
        _ => None,
    }
}

/// [`body_refs`], None when the body is bad.
pub(crate) fn body_refs_if_good(c: &Cst, body: NodeId) -> Option<HashSet<NodeId>> {
    let mut used: HashSet<NodeId> = HashSet::new();
    let in_decl = |e: NodeId| {
        crate::support::slang_ast::ancestors(c, e).take_while(|&a| a != body).any(|a| c.kind(a) == Declarator)
    };
    let all = crate::support::slang_ast::subtree(c, body);
    let decls: Vec<(u32, u32)> =
        all.iter().filter(|&&d| c.kind(d) == DataDecl).filter_map(|&d| c.tree.token_bounds(d)).collect();
    let names: Vec<NodeId> = all
        .iter()
        .copied()
        .filter(|&e| c.kind(e) == NameExpr && kids(c, e).is_empty() && !is_local_qualifier(c, e))
        .collect();
    let bad = c.tree.token_bounds(body).is_some_and(|(a, b)| {
        crate::support::sema_facts::bad_ranges(c)
            .into_iter()
            .any(|(x, y)| a <= x && y <= b && !decls.iter().any(|&(p, q)| p <= x && y <= q))
    }) || names.iter().any(|&e| !in_decl(e) && !crate::support::sema_facts::expr_binds(c, e))
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
pub(crate) fn reads_bad_typed_local(c: &Cst, e: NodeId, body: NodeId) -> bool {
    let Some(t) = c.first_tok(e) else { return false };
    let Some(d) = lookup(c, e, c.text_of(t)) else { return false };
    let Some(dd) = crate::support::slang_ast::ancestors(c, d.node).find(|&a| c.kind(a) == DataDecl) else {
        return false;
    };
    if !crate::support::slang_ast::ancestors(c, dd).any(|a| a == body) {
        return false;
    }
    let Some((a, b)) = c.tree.child(dd, DataType).and_then(|ty| c.tree.token_bounds(ty)) else { return false };
    crate::support::sema_facts::bad_ranges(c).into_iter().any(|(x, y)| a <= x && y <= b)
}

/// The `local` of a `local::x` name.
pub(crate) fn is_local_qualifier(c: &Cst, e: NodeId) -> bool {
    c.first_tok(e).is_some_and(|t| c.text_of(t) == "local") && c.parent(e).is_some_and(|p| c.kind(p) == ScopedExpr)
}

/// `name` at `e` is inside the `with` constraint of `obj.randomize()` and
/// names a member of obj's class, which constraint names resolve to first.
pub(crate) fn randomize_with_member(c: &Cst, e: NodeId, name: &str, stop: NodeId) -> bool {
    for w in crate::support::slang_ast::ancestors(c, e).take_while(|&a| a != stop) {
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
            crate::support::slang_ast::enclosing_class(c, e)
        } else {
            expr_class(c, base)
        };
        if cls.is_some_and(|k| crate::support::slang_ast::class_member(c, k, name).is_some()) {
            return true;
        }
    }
    false
}

/// `name` at `e` is a loop variable of a `foreach` around it.
pub(crate) fn foreach_var_in_scope(c: &Cst, e: NodeId, name: &str, stop: NodeId) -> bool {
    crate::support::slang_ast::ancestors(c, e).take_while(|&a| a != stop).any(|l| {
        if c.kind(l) != LoopStmt || !c.keyword(l).is_some_and(|k| c.text_of(k) == "foreach") {
            return false;
        }
        let toks: Vec<&str> = c.toks(l).map(|t| c.text_of(t)).collect();
        let Some(open) = toks.iter().position(|t| *t == "[") else { return false };
        let close = toks.iter().position(|t| *t == ")").unwrap_or(toks.len());
        toks[open..close].contains(&name)
    })
}

/// The class's declaration of method `name` (prototype or body).
pub(crate) fn method_decl_in(c: &Cst, cls: NodeId, name: &str) -> Option<NodeId> {
    kids(c, cls).into_iter().find(|&m| {
        matches!(c.kind(m), FunctionDecl | TaskDecl | MethodPrototype)
            && crate::support::slang_ast::last_name(c, m) == name
    })
}

/// slang's `isVirtual()`: declared virtual (on the body or its
/// prototype), or overriding a virtual method of a base class.
pub(crate) fn routine_is_virtual(c: &Cst, r: NodeId) -> bool {
    use crate::support::slang_ast::{class_chain, enclosing_class, has_kw, last_name};
    if has_kw(c, r, "virtual") {
        return true;
    }
    let Some(cls) = enclosing_class(c, r) else { return false };
    if !crate::support::slang_ast::ancestors(c, r).any(|a| a == cls) && c.kind(r) != MethodPrototype {
        // Out of block: the prototype says.
        let name = last_name(c, r);
        if method_decl_in(c, cls, &name).is_some_and(|p| has_kw(c, p, "virtual")) {
            return true;
        }
    }
    let name = last_name(c, r);
    class_chain(c, cls)
        .into_iter()
        .skip(1)
        .any(|b| method_decl_in(c, b, &name).is_some_and(|m| has_kw(c, m, "virtual")))
}
