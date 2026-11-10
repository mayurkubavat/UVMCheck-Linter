//! SV-BUG-003 (SelfAssignment): Flags 'x = x' assignments, typically a
//! constructor or setter whose argument shadows the property it was meant to
//! set (clang-tidy bugprone-self-assignment, Sonar S1656).

use crate::support::class_items::{declarators, kids, op, user_code};
use crate::support::data_items::strip_conv;
use crate::support::sema_facts::ast_ok;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

/// `x = x;` (blocking, plain `=`) where x is a name or a member access.
pub(crate) fn self_assignment(c: &Cst, s: &mut Sink) {
    for n in c.nodes(BinaryExpr) {
        if op(c, n) != "=" || !user_code(c, n) {
            continue;
        }
        let k = kids(c, n);
        let (Some(&l), Some(&r)) = (k.first(), k.get(1)) else { continue };
        if !matches!(c.kind(l), NameExpr | MemberExpr) {
            continue;
        }
        let (r, _) = strip_conv(c, r);
        let name = c.tree.bare_text(l);
        // slang binds `this.x` and `super.x` to the member itself.
        fn member(t: &str) -> &str {
            let mut t = t;
            while let Some(r) = t.strip_prefix("this.").or_else(|| t.strip_prefix("super.")) {
                t = r;
            }
            t
        }
        let rname = c.tree.bare_text(r);
        // A plain name next to `this.x` is the member only when no formal
        // or local of the routine takes the name.
        let plain_is_member = |t: &str| member(t) != t || !routine_declares(c, n, t);
        if name.is_empty()
            || member(&name) != member(&rname)
            || (name != rname && !(plain_is_member(&name) && plain_is_member(&rname)))
            || !matches!(c.kind(r), NameExpr | MemberExpr)
        {
            continue;
        }
        if !ast_ok(c, n) {
            continue;
        }
        let mut hint = String::new();
        if c.kind(l) == NameExpr && is_formal(c, n, &name) {
            hint = format!(
                " The argument '{name}' hides the class property of the same name; write 'this.{name} = {name};'."
            );
        }
        let Some(at) = c.first_tok(n) else { continue };
        s.add(
            "SV-BUG-003",
            "SelfAssignment",
            Severity::Warning,
            format!("'{name}' is assigned to itself, so the statement has no effect.{hint}"),
            c.loc(at),
        );
    }
}

/// `name` at `n` names a formal argument of the routine around it (and no
/// local of its body).
/// The routine around `n` declares `name` as a formal or a local.
fn routine_declares(c: &Cst, n: NodeId, name: &str) -> bool {
    let mut x = n;
    while let Some(p) = c.parent(x) {
        if matches!(c.kind(p), FunctionDecl | TaskDecl) {
            let declares = |kind, holder: Option<NodeId>| {
                holder.is_some_and(|h| {
                    c.tree
                        .children(h)
                        .filter(|&d| c.kind(d) == kind)
                        .any(|d| declarators(c, d).iter().any(|(_, v, _)| v == name))
                })
            };
            return declares(DataDecl, c.tree.child(p, Body)) || declares(Port, c.tree.child(p, PortList));
        }
        x = p;
    }
    false
}

fn is_formal(c: &Cst, n: NodeId, name: &str) -> bool {
    let mut x = n;
    while let Some(p) = c.parent(x) {
        if matches!(c.kind(p), FunctionDecl | TaskDecl) {
            let declares = |kind, holder: Option<NodeId>| {
                holder.is_some_and(|h| {
                    c.tree
                        .children(h)
                        .filter(|&d| c.kind(d) == kind)
                        .any(|d| declarators(c, d).iter().any(|(_, v, _)| v == name))
                })
            };
            if declares(DataDecl, c.tree.child(p, Body)) {
                return false;
            }
            return declares(Port, c.tree.child(p, PortList));
        }
        x = p;
    }
    false
}
