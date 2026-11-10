//! SV-DSN-006 (OverrideOnlyCallsSuper): Flags overrides whose whole body is the
//! super call with the same arguments, such as a connect_phase that only calls
//! super.connect_phase(phase) (Pylint useless-parent-delegation, Sonar S1185).

use crate::support::class_index::Classes;
use crate::support::statements::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

/// The only statement of a body, looking through begin/end.
fn sole_statement(c: &Cst, n: NodeId) -> Option<NodeId> {
    let mut s = n;
    loop {
        match c.kind(s) {
            ParBlock => return Some(s),
            Body | SeqBlock => {
                let items: Vec<NodeId> = list(c, s).into_iter().filter(|&x| c.kind(x) != EmptyStmt).collect();
                if items.len() != 1 {
                    return None;
                }
                s = items[0];
            }
            _ => return Some(s),
        }
    }
}

pub(crate) fn override_only_calls_super(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    for i in classes.user() {
        let cls = &classes.list[i];
        for m in &cls.methods {
            if m.name == "new" {
                continue;
            }
            let Some(body) = m.body else { continue };
            let Some(st) = sole_statement(c, body) else { continue };
            let call = match c.kind(st) {
                ExprStmt => exprs(c, st).into_iter().next(),
                ReturnStmt => exprs(c, st).into_iter().next(),
                _ => None,
            };
            let Some(call) = call.map(|x| strip_parens(c, x)) else {
                continue;
            };
            if c.kind(call) != CallExpr || call_name(c, call) != Some(m.name.as_str()) {
                continue;
            }
            let callee = c.tree.children(call).next();
            if !callee.is_some_and(|x| c.kind(x) == MemberExpr && c.tree.bare_text(x).starts_with("super.")) {
                continue;
            }
            // `super.m` must bind to a base-class method (not in a class
            // whose base is unknown, e.g. uvm_pkg missing).
            if !classes.ancestor_has(i, &m.name) {
                continue;
            }
            let actual = call_args(c, call);
            if actual.len() != m.ports.len() {
                continue;
            }
            let same = actual.iter().zip(&m.ports).all(|(a, p)| {
                a.is_some_and(|a| {
                    let a = strip_parens(c, a);
                    let a = assign_expr(c, a).map(|(l, _, _)| l).unwrap_or(a);
                    ident(c, a) == Some(p.name.as_str())
                })
            });
            if !same {
                continue;
            }
            s.add(
                "SV-DSN-006",
                "OverrideOnlyCallsSuper",
                Severity::Info,
                format!(
                    "'{}::{}' only calls super.{}() with the same arguments. The override adds nothing; delete it.",
                    cls.name, m.name, m.name
                ),
                c.loc(m.tok),
            );
        }
    }
}
