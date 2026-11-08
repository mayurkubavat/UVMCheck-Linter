//! SV-DRV-001 (DrivingDuringReset): Disallow driving virtual interface control
//! signals without an active reset check or synchronization guard.

use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn driving_during_reset(c: &Cst, s: &mut Sink) {
    for t in c.nodes(TaskDecl) {
        let Some(kw) = c.tok(t, "task") else { continue };
        let Some((_, last)) = c.tree.token_bounds(t) else { continue };
        let text = span_text(c, kw, last);
        let drives = (text.contains("vif.") || text.contains("intf.")) && text.contains("<=");
        if !drives {
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
