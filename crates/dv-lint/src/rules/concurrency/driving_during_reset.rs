//! SV-DRV-001 (DrivingDuringReset): Disallow driving virtual interface control
//! signals without an active reset check or synchronization guard.

use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind;
use SyntaxKind::*;

/// Whether task `t` has a non-blocking drive of an interface signal: by the
/// targets' declarations ([`crate::sem::reach`]), and by the names `vif.`
/// and `intf.` for a task with a target that does not resolve.
fn drives_interface(c: &Cst, t: sv_syntax::NodeId, text: &str) -> bool {
    let mut unresolved = false;
    for n in c.subtree(t) {
        if c.kind(n) != BinaryExpr || c.parent(n).map(|p| c.kind(p)) != Some(ExprStmt) || c.tok(n, "<=").is_none() {
            continue;
        }
        match c.tree.children(n).next().and_then(|l| crate::sem::reach(c, l)) {
            Some(r) if r.driven_from_testbench() => return true,
            Some(_) => {}
            None => unresolved = true,
        }
    }
    unresolved && (text.contains("vif.") || text.contains("intf.")) && text.contains("<=")
}

pub(crate) fn driving_during_reset(c: &Cst, s: &mut Sink) {
    for t in c.nodes(TaskDecl) {
        let Some(kw) = c.tok(t, "task") else { continue };
        let Some((_, last)) = c.tree.token_bounds(t) else { continue };
        let text = span_text(c, kw, last);
        if !drives_interface(c, t, &text) {
            continue;
        }
        let mut clean = text.clone();
        while let Some(p) = clean.find("//") {
            let eol = clean[p..].find('\n').map_or(clean.len(), |e| p + e);
            clean.replace_range(p..eol, "");
        }
        let lower = clean.to_lowercase();
        if lower.contains("rst") || lower.contains("reset") {
            continue;
        }
        let Some(nm) = c.tree.child(t, Name) else { continue };
        let Some(ft) = c.first_tok(nm) else { continue };
        let name = c.text_of(ft);
        s.add(
            "SV-DRV-001",
            "DrivingDuringReset",
            Severity::Warning,
            format!(
                "Task '{name}' drives virtual interface signals without an active reset check or synchronization guard. \
                 Driving protocol control signals while hardware reset is asserted causes bus protocol violations \
                 and synchronizer state corruption. Guard signal driving with 'wait(vif.rst_n == 1)' or 'if (!vif.rst_n)'."
            ),
            c.loc(ft),
        );
    }
}
