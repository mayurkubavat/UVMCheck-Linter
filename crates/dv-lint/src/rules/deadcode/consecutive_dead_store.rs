//! SV-DED-004 (ConsecutiveDeadStore): Detects consecutive assignments to the
//! same variable where the initial value is immediately overwritten without
//! being read.

use crate::support::statements::{
    ancestors_or_self, assignment, has_error, ident, list, report, start, statement_lists, subtree, Units,
};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

pub(crate) fn dead_store(c: &Cst, s: &mut Sink) {
    let units = Units::new(c);
    let lists = statement_lists(c);
    let poisoned = bad_bodies(c, &lists);
    for b in lists {
        if ancestors_or_self(c, b).any(|x| poisoned.contains(&x.0)) || crate::support::sema_facts::holds_bad(c, b) {
            continue;
        }
        let items = list(c, b);
        for w in items.windows(2) {
            let Some((l1, _, _)) = assignment(c, w[0]) else {
                continue;
            };
            let Some(n1) = store_target(c, l1) else { continue };
            let Some((l2, r2, _)) = assignment(c, w[1]) else {
                continue;
            };
            if store_target(c, l2) != Some(n1) {
                continue;
            }
            // slang drops invalid statements: a syntax error, or a compound
            // assignment to an enum variable.
            if has_error(c, w[0]) || has_error(c, w[1]) {
                continue;
            }
            let reads = subtree(c, r2).into_iter().any(|x| reads_name(c, x, n1));
            if !reads {
                report(
                    s,
                    units.times(c, b),
                    "SV-DED-004",
                    "ConsecutiveDeadStore",
                    Severity::Warning,
                    format!(
                        "Dead store to variable '{n1}'. \
                         The assigned value is unconditionally overwritten by the next assignment statement before ever being read."
                    ),
                    start(c, w[0]),
                );
            }
        }
    }
}

/// The variable an assignment target names as slang binds it: a plain name,
/// or `super.x`, which slang binds straight to the inherited member (unlike
/// `this.x`, a member access).
fn store_target<'t>(c: &Cst<'t>, e: NodeId) -> Option<&'t str> {
    if c.kind(e) == MemberExpr {
        let head = c.tree.children(e).next()?;
        if c.kind(head) != NameExpr || c.tree.bare_text(head) != "super" {
            return None;
        }
        let t = c.toks(e).last()?;
        return Some(c.text_of(t));
    }
    ident(c, e)
}

/// Outermost statements of procedural bodies that slang cannot bind. A bad
/// statement makes its whole statement list bad, up to the body, and the
/// AST walk skips it. Detected: a compound assignment to an enum variable.
fn bad_bodies(c: &Cst, lists: &[NodeId]) -> std::collections::HashSet<u32> {
    let mut out = std::collections::HashSet::new();
    for &b in lists {
        for st in list(c, b) {
            let Some((l, _, op)) = assignment(c, st) else { continue };
            if op == "=" {
                continue;
            }
            let Some(name) = ident(c, l) else { continue };
            if !enum_typed(c, b, name) {
                continue;
            }
            let mut root = st;
            while let Some(p) = c.parent(root) {
                if matches!(c.kind(p), ProceduralBlock | FunctionDecl | TaskDecl | ClassDecl | ModuleDecl) {
                    break;
                }
                root = p;
            }
            out.insert(root.0);
        }
    }
    out
}

/// The nearest declaration of `name` around `n` has an enum type.
fn enum_typed(c: &Cst, n: NodeId, name: &str) -> bool {
    let is_enum = |ty: NodeId| -> bool {
        let Some(t) = c.first_tok(ty) else { return false };
        if c.text_of(t) == "enum" {
            return true;
        }
        c.nodes(TypedefDecl).into_iter().any(|td| {
            c.tree.child(td, Name).is_some_and(|nm| c.tree.bare_text(nm) == c.text_of(t))
                && c.tree.child(td, DataType).and_then(|d| c.first_tok(d)).is_some_and(|f| c.text_of(f) == "enum")
        })
    };
    let mut x = Some(n);
    while let Some(y) = x {
        for d in c.tree.children(y).filter(|&k| c.kind(k) == DataDecl) {
            let declares = c
                .tree
                .children(d)
                .filter(|&k| c.kind(k) == Declarator)
                .any(|k| c.tree.child(k, Name).is_some_and(|nm| c.tree.bare_text(nm) == name));
            if declares {
                return c.tree.child(d, DataType).is_some_and(is_enum);
            }
        }
        x = c.parent(y);
    }
    false
}

/// A NamedValue reference to `name`: a bare identifier that is not the
/// member name of `a.b` nor a callee.
fn reads_name(c: &Cst, x: NodeId, name: &str) -> bool {
    if store_target(c, x) != Some(name) {
        return false;
    }
    // The callee of `name(...)` is a call, not a value.
    !c.parent(x).is_some_and(|p| c.kind(p) == CallExpr && c.tree.children(p).next() == Some(x))
}
