//! UVM-RAL-001 (DirectRalMirrorTampering): Direct assignment to internal RAL
//! fields (m_value, m_mirrored, m_desired) bypasses register callbacks, access
//! policy checks, and prediction pipelines.

use crate::support::uvm_classes::{assign_parts, derived, is_name_chain, At, Ix};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use sv_syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn ral_mirror_tampering(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for (ti, c) in csts.iter().enumerate() {
        for n in c.nodes(BinaryExpr) {
            let Some((lhs, _)) = assign_parts(c, n) else { continue };
            if c.kind(lhs) != MemberExpr {
                continue;
            }
            let member = c.last_text(lhs);
            if !matches!(member, "m_value" | "m_mirrored" | "m_desired") {
                continue;
            }
            let Some(value) = c.tree.children(lhs).next() else { continue };
            let cx = ix.ctx(At { tree: ti, node: n });
            let Some(k) = ix.expr_class(c, &cx, value, 0) else { continue };
            let tn = &d.class(k).name;
            let ral = tn.contains("reg")
                || tn.contains("field")
                || derived(d, k, "uvm_reg")
                || derived(d, k, "uvm_reg_field")
                || derived(d, k, "uvm_reg_block");
            if !ral {
                continue;
            }
            let reg = if is_name_chain(c, value) { "reg".to_string() } else { c.text(value) };
            let Some(t) = c.first_tok(lhs) else { continue };
            s.add(
                "UVM-RAL-001",
                "DirectRalMirrorTampering",
                Severity::Error,
                format!(
                    "Direct assignment to internal RAL property '{member}' on register/field handle '{reg}'. \
                     Bypassing the standard UVM RAL API avoids access policy enforcement (RO, WO, W1C), \
                     prediction callbacks, coverage sampling, and mirror consistency checks. \
                     Use 'predict()' to update the mirrored model state, or execute bus read/write operations."
                ),
                c.loc(t),
            );
        }
    }
}
