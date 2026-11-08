//! UVM-ROL-001 (DriverGeneratesStimulus): Flags drivers that randomize sequence
//! items or start sequences.

use crate::support::class_roles::{role, role_name, Ctx, Index, Role};
use crate::support::routine_bodies::{bodies, calls_in};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

/// The object a call works on: `obj` of `obj.f()`, the first argument of
/// `std::randomize(obj)`.
fn receiver_of(c: &Cst, call: NodeId) -> Option<NodeId> {
    let callee = c.tree.children(call).next()?;
    match c.kind(callee) {
        MemberExpr => c.tree.children(callee).next(),
        ScopedExpr if c.tree.bare_text(callee) == "std::randomize" => {
            c.arg_expr(c.call_args(call).first().copied().flatten())
        }
        _ => None,
    }
}

/// `exprText(*receiverOf(call))`: slang's receiver of `obj.f()` has no
/// syntax of its own, so only `std::randomize(obj)` names it.
fn receiver_text(c: &Cst, call: NodeId, r: NodeId) -> String {
    let callee = c.tree.children(call).next();
    if callee.is_some_and(|x| c.kind(x) == MemberExpr) {
        String::new()
    } else {
        c.expr_text(r)
    }
}

pub(crate) fn driver_generates_stimulus(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for (cls, m, c, body) in bodies(d, &ix) {
        if role(d, cls) != Role::Driver {
            continue;
        }
        let ctx = Ctx { d, cls, m: Some(m), td: Some(&ix.typedefs) };
        for call in calls_in(c, body) {
            let n = c.callee_name(call);
            if n != "randomize" && n != "start" {
                continue;
            }
            let Some(r) = receiver_of(c, call) else { continue };
            let rr = ctx.class_of(c, r).map(|k| role(d, k)).unwrap_or(Role::None);
            let what = if n == "randomize" && (rr == Role::Item || rr == Role::Sequence) {
                format!("randomizes {} '{}'", role_name(rr), receiver_text(c, call, r))
            } else if n == "start" && rr == Role::Sequence {
                format!("starts sequence '{}'", receiver_text(c, call, r))
            } else {
                continue;
            };
            out.add(
                "UVM-ROL-001",
                "DriverGeneratesStimulus",
                Severity::Warning,
                format!(
                    "Driver '{}' {} in {}(). Stimulus generated inside the driver cannot be constrained or replaced by tests; move it into a sequence and let the \
driver only drive the items it receives.",
                    cls.name, what, m.name
                ),
                c.loc_of(call),
            );
        }
    }
}
