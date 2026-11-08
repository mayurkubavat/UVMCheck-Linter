//! SV-RND-027 (SoftConstraintShadowing): Flags soft constraints whose winner is
//! hard to predict: 'soft' inside an inline randomize() with { } block, and a
//! soft constraint on a variable that a base class or another constraint block
//! of the same class already constrains softly.

use crate::support::class_roles::{user_classes, Ctx, Index};
use crate::support::routine_bodies::is_with_wrapper;
use crate::support::routine_owner::{bad_body, free_routine_names, in_library, routine_class};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::{ClassId, Design};
use std::collections::{HashMap, HashSet};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

/// The expressions of `soft` constraints under a constraint block or item,
/// through lists, implications, conditionals and foreach.
fn soft_exprs(c: &Cst, n: NodeId, out: &mut Vec<NodeId>) {
    match c.kind(n) {
        ConstraintBlock => {
            for k in c.node_kids(n) {
                soft_exprs(c, k, out);
            }
        }
        ConstraintItem => {
            let kw = c.keyword(n).map(|t| c.text_of(t)).unwrap_or("");
            if kw == "soft" {
                if let Some(e) = c.tree.children(n).next() {
                    out.push(e);
                }
                return;
            }
            if kw == "if" || kw == "foreach" || c.tok(n, "->").is_some() {
                for k in c.node_kids(n) {
                    if matches!(c.kind(k), ConstraintItem | ConstraintBlock) {
                        soft_exprs(c, k, out);
                    }
                }
            }
        }
        _ => {}
    }
}

/// Rand properties named in an expression, in the order they appear.
fn rand_vars(ctx: &Ctx, c: &Cst, e: NodeId) -> Vec<(ClassId, String)> {
    let mut out: Vec<(ClassId, String)> = Vec::new();
    let mut push = |v: (ClassId, String)| {
        if !out.contains(&v) {
            out.push(v);
        }
    };
    for n in c.subtree(e) {
        let (owner, name) = match c.kind(n) {
            NameExpr if c.tree.children(n).next().is_none() => (ctx.cls, c.tree.bare_text(n)),
            MemberExpr => {
                let Some(obj) = c.tree.children(n).next() else { continue };
                let oc = if c.kind(obj) == NameExpr && c.tree.bare_text(obj) == "this" {
                    Some(ctx.cls)
                } else {
                    ctx.class_of(c, obj)
                };
                let Some(oc) = oc else { continue };
                (oc, c.last_text(n).to_string())
            }
            _ => continue,
        };
        let found = std::iter::once(owner)
            .chain(ctx.d.ancestors(owner.id))
            .find_map(|k| k.properties.iter().find(|p| p.name == name).map(|p| (k, p)));
        if let Some((k, p)) = found {
            if p.is_rand {
                push((k.id, name));
            }
        }
    }
    out
}

/// A class's constraint blocks, in member order: (name, tree, block). A
/// prototype (`constraint c;`) has its out-of-block body.
fn constraint_blocks<'a, 't>(ix: &Index<'a, 't>, c: &'a Cst<'t>, decl: NodeId) -> Vec<(String, &'a Cst<'t>, NodeId)> {
    let cname = c.tree.child(decl, Name).map(|n| c.tree.bare_text(n)).unwrap_or_default();
    let mut out = Vec::new();
    for x in c.items(decl) {
        let name = c.tree.child(x, Name).map(|n| c.tree.bare_text(n)).unwrap_or_default();
        match c.kind(x) {
            ConstraintDecl => {
                if let Some(b) = c.tree.child(x, ConstraintBlock) {
                    out.push((name, c, b));
                }
            }
            ConstraintPrototype => {
                let full = format!("{cname}::{name}");
                let body = ix.csts.iter().find_map(|k| {
                    k.first_named(ConstraintDecl, &full).and_then(|n| k.tree.child(n, ConstraintBlock)).map(|b| (k, b))
                });
                if let Some((k, b)) = body {
                    out.push((name, k, b));
                }
            }
            _ => {}
        }
    }
    out
}

/// The routine body around a node is bad (see [`bad_body`]).
fn in_bad_body(d: &Design, c: &Cst, n: NodeId, free: &HashSet<String>) -> bool {
    let mut p = c.parent(n);
    while let Some(x) = p {
        if c.kind(x) == Body {
            let cls = c.parent(x).and_then(|decl| routine_class(d, c, decl));
            return bad_body(d, c, x, cls, free);
        }
        p = c.parent(x);
    }
    false
}

pub(crate) fn soft_constraint_shadowing(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for cls in user_classes(d) {
        let Some((c, decl)) = ix.decl(cls.loc) else { continue };
        if c.kind(decl) != ClassDecl {
            continue;
        }
        // Soft targets of every user base class, nearest first.
        let mut inherited: HashMap<(ClassId, String), String> = HashMap::new();
        for b in d.ancestors(cls.id) {
            if crate::support::class_roles::is_library(d, b) {
                break;
            }
            let Some((bc, bdecl)) = ix.decl(b.loc) else { continue };
            let bctx = Ctx { d, cls: b, m: None, td: Some(&ix.typedefs) };
            for (_, bk, blk) in constraint_blocks(&ix, bc, bdecl) {
                let mut es = Vec::new();
                soft_exprs(bk, blk, &mut es);
                for e in es {
                    for v in rand_vars(&bctx, bk, e) {
                        inherited.entry(v).or_insert_with(|| b.name.clone());
                    }
                }
            }
        }
        let ctx = Ctx { d, cls, m: None, td: Some(&ix.typedefs) };
        let mut seen: HashMap<(ClassId, String), usize> = HashMap::new();
        let blocks = constraint_blocks(&ix, c, decl);
        for (bi, &(_, bk, blk)) in blocks.iter().enumerate() {
            let mut es = Vec::new();
            soft_exprs(bk, blk, &mut es);
            for e in es {
                for v in rand_vars(&ctx, bk, e) {
                    let other = if let Some(b) = inherited.get(&v) {
                        format!("base class '{b}'")
                    } else if let Some(&s) = seen.get(&v).filter(|&&s| s != bi) {
                        format!("constraint '{}'", blocks[s].0)
                    } else {
                        String::new()
                    };
                    seen.entry(v.clone()).or_insert(bi);
                    if other.is_empty() {
                        continue;
                    }
                    out.add(
                        "SV-RND-027",
                        "SoftConstraintShadowing",
                        Severity::Warning,
                        format!(
                            "'{}' is also constrained softly in {other}. Soft constraints resolve by declaration order, so which default wins \
is hidden from the reader. Keep one soft default and override it with a hard constraint.",
                            v.1
                        ),
                        bk.loc_of(e),
                    );
                    break;
                }
            }
        }
    }
    // `soft` inside randomize() with { } anywhere in user code.
    let free = free_routine_names(csts);
    for c in csts {
        for n in c.nodes(CallExpr) {
            if !is_with_wrapper(c, n) {
                continue;
            }
            let Some(inner) = c.tree.children(n).next() else { continue };
            if c.callee_name(inner) != "randomize" {
                continue;
            }
            let Some(blk) = c.tree.child(n, WithClause).and_then(|w| c.tree.child(w, ConstraintBlock)) else {
                continue;
            };
            if in_library(d, c, n) || in_bad_body(d, c, n, &free) {
                continue;
            }
            let mut es = Vec::new();
            soft_exprs(c, blk, &mut es);
            for e in es {
                out.add(
                    "SV-RND-027",
                    "SoftConstraintShadowing",
                    Severity::Warning,
                    "'soft' in an inline randomize() with { } block. The caller's intent is a requirement here; a soft inline constraint quietly loses to any class \
constraint, so readers cannot tell what value they will get. Drop 'soft'."
                        .into(),
                    c.loc_of(e),
                );
            }
        }
    }
}
