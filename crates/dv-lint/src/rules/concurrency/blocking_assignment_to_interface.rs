//! SV-INT-001 (BlockingAssignmentToInterface): Disallows blocking assignments
//! ('=') when driving virtual interface or clocking block signals from
//! procedural code.
//!
//! Not in the UVM phases that run before simulation time starts
//! (build, connect, end_of_elaboration, start_of_simulation): writing a
//! configuration bit of an interface there (`cfg.vif.if_mode = ...`, as
//! OpenTitan's agents do) races with nothing. Nor for a bare `cb.x` in a
//! class, where `cb` cannot be a clocking block. The C++ rule is the same.

/// The text of an assignment target that is a clocking block or virtual
/// interface signal (shared with SV-LANG-005).
pub(crate) fn interface_signal(lhs: &str) -> bool {
    lhs.contains(".cb.") || lhs.starts_with("cb.") || lhs.contains("vif.") || lhs.contains("_vif.")
}

/// A bare `cb.x` in a class method: a clocking block cannot be declared in
/// a class, so there `cb` is a handle or a variable (UVM's own
/// `for (uvm_vreg_cbs cb = cbs.first(); ...) cb.fname = ...`). A clocking
/// block reached from a class goes through the interface (`vif.cb.x`).
pub(crate) fn handle_named_cb(c: &Cst, n: sv_syntax::NodeId, lhs: &str) -> bool {
    lhs.starts_with("cb.")
        && !lhs.contains(".cb.")
        && !lhs.contains("vif.")
        && crate::support::slang_ast::enclosing_class(c, n).is_some()
}

/// A function phase that runs before time 0 ends: `build_phase`, also as
/// `cls::build_phase` out of its class.
pub(crate) fn in_setup_phase(c: &Cst, n: sv_syntax::NodeId) -> bool {
    crate::support::slang_ast::ancestors(c, n).find(|&a| matches!(c.kind(a), FunctionDecl | TaskDecl)).is_some_and(
        |f| {
            c.kind(f) == FunctionDecl
                && c.tree.child(f, Name).is_some_and(|nm| {
                    let name = c.tree.bare_text(nm);
                    let name = name.rsplit("::").next().unwrap_or(&name).trim().to_string();
                    matches!(
                        name.as_str(),
                        "build_phase" | "connect_phase" | "end_of_elaboration_phase" | "start_of_simulation_phase"
                    )
                })
        },
    )
}

use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn blocking_to_interface(c: &Cst, s: &mut Sink) {
    for b in c.nodes(BinaryExpr) {
        let Some(op) = c.keyword(b) else { continue };
        if c.text_of(op) != "=" {
            continue;
        }
        // A continuous assignment drives the signal outside any process, so
        // there is no Active-region race to avoid, and `assign x <= y` is not
        // legal SystemVerilog.
        if crate::support::slang_ast::ancestors(c, b).any(|a| c.kind(a) == ContinuousAssign) {
            continue;
        }
        let Some(&l) = kids(c, b).first() else { continue };
        let lhs = slang_trimmed(c, l);
        if interface_signal(&lhs) && !handle_named_cb(c, b, &lhs) && !in_setup_phase(c, b) {
            s.add(
                "SV-INT-001",
                "BlockingAssignmentToInterface",
                Severity::Error,
                format!(
                    "Blocking assignment '=' used to drive clocking block or interface signal '{lhs}'. In SystemVerilog (IEEE 1800 § 14.16), clocking block and interface signals must be driven using non-blocking clocking drive ('<=') to avoid simulation race conditions with DUT Active-region logic."
                ),
                c.loc(op),
            )
            .fixes
            .extend({
                // A function may not hold '<=' cleanly either (SV-LANG-005
                // would undo the fix), so there the rewrite is only shown.
                let in_function = crate::support::slang_ast::ancestors(c, b).any(|a| c.kind(a) == FunctionDecl);
                let (desc, safety) = if in_function {
                    ("Move the drive into a task and use non-blocking '<='", crate::fix::FixSafety::DisplayOnly)
                } else {
                    ("Replace blocking assignment '=' with non-blocking clocking drive '<='", crate::fix::FixSafety::Unsafe)
                };
                crate::fix::with_safety(c.token_fix(op, op, "<=", desc), safety)
            });
        }
    }
}
