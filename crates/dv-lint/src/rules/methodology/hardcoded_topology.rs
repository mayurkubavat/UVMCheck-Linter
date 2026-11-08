//! UVM-RUS-003 (HardcodedTopology): Flags envs that create components in a loop
//! with a literal count instead of a count from their config object
//! (open/closed principle, MG-4.2).

use dv_model::Design;
use sv_syntax::{Child, NodeId};
use crate::support::class_roles::{is_component_role, role, Ctx, Index, Role};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;
use crate::support::routine_bodies::{Calls, bodies, creations, integer_literal};

/// A loop: its keyword, the slots between its parentheses (split at `;`)
/// and its body.
pub(crate) fn loop_parts<'t>(c: &Cst<'t>, n: NodeId) -> (&'t str, Vec<Option<NodeId>>, Option<NodeId>) {
    let kw = c.keyword(n).map(|t| c.text_of(t)).unwrap_or("");
    let mut slots: Vec<Option<NodeId>> = vec![None];
    let mut depth = 0;
    let mut body = None;
    let mut closed = false;
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
                ";" if depth == 1 && !closed => slots.push(None),
                _ => {}
            },
            Child::Node(x) => {
                if matches!(c.kind(x), StmtLabel | Attribute) {
                    continue;
                }
                if closed || (depth == 0 && kw == "forever") {
                    body = Some(x);
                } else if let Some(s @ None) = slots.last_mut() {
                    *s = Some(x);
                }
            }
        }
    }
    (kw, slots, body)
}

pub(crate) fn hardcoded_topology(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for (cls, m, c, body) in bodies(d, &ix) {
        if role(d, cls) != Role::Env {
            continue;
        }
        let ctx = Ctx { d, cls, m: Some(m), td: Some(&ix.typedefs) };
        let calls = Calls::new(m);
        for l in c.subtree(body).into_iter().filter(|&x| c.kind(x) == LoopStmt) {
            let (kw, slots, lbody) = loop_parts(c, l);
            let bound = match kw {
                "repeat" => slots.first().copied().flatten().and_then(|e| integer_literal(c, e)),
                "for" => slots.get(1).copied().flatten().and_then(|stop| {
                    let stop = c.strip(stop);
                    (c.kind(stop) == BinaryExpr)
                        .then(|| c.tree.children(stop).nth(1))
                        .flatten()
                        .and_then(|r| integer_literal(c, r))
                }),
                _ => None,
            };
            let (Some(bound), Some(lbody)) = (bound, lbody) else { continue };
            if let Some(cr) = creations(&ctx, &calls, c, lbody).into_iter().find(|cr| is_component_role(role(d, cr.class))) {
                out.add(
                    "UVM-RUS-003",
                    "HardcodedTopology",
                    Severity::Warning,
                    format!(
                        "Env '{}' creates {} instances of '{}' with a literal loop count. The same env cannot then be reused with a different number of interfaces; take the count \
(and per-instance settings) from the env's config object.",
                        cls.name, bound, cr.class.name
                    ),
                    c.loc_of(l),
                );
            }
        }
    }
}
