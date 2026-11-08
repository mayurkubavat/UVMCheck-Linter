//! RAL-ACC-001 (DirectMirrorSetWithoutWrite): Flags direct reg.set(...) calls
//! that update only software mirrors without issuing physical DUT write
//! transactions.

use crate::support::uvm_classes::{receiver, unparen};
use crate::support::uvm_syntax::callee;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use sv_syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn direct_mirror_set(_d: &Design, csts: &[Cst], s: &mut Sink) {
    for c in csts {
        for n in c.nodes(CallExpr) {
            if callee(c, n) != "set" {
                continue;
            }
            let Some(recv) = receiver(c, n) else { continue };
            let recv = unparen(c, recv);
            let target = match c.kind(recv) {
                NameExpr => c.tree.bare_text(recv),
                MemberExpr => c.last_text(recv).to_string(),
                // Slang's element select has no syntax: the name stays "reg".
                SelectExpr => "reg".to_string(),
                _ => c.trimmed(recv),
            };
            if !(target.contains("reg")
                || target.contains("field")
                || target.contains("model")
                || target.contains("ral"))
            {
                continue;
            }
            let Some(t) = c.first_tok(n) else { continue };
            s.add(
                "RAL-ACC-001",
                "DirectMirrorSetWithoutWrite",
                Severity::Warning,
                format!(
                    "Direct mirror update '{target}.set(...)' modifies only the local software mirror table without issuing a bus transaction to the physical DUT. In UVM RAL architecture (IEEE 1800.2 § 18.5), invoke 'write()' or follow with 'update()' to program physical hardware and maintain register state synchronization."
                ),
                c.loc(t),
            );
        }
    }
}
