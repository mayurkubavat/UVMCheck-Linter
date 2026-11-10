//! UVM-TLM-005 (AnalysisTransactionMutation): Detects in-place modification of
//! transactions received via analysis write() methods.

use crate::support::uvm_classes::assign_parts;
use crate::support::uvm_syntax::{name_node, routine_last_name, subtree};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use svlang::syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn analysis_mutation(_d: &Design, csts: &[Cst], s: &mut Sink) {
    for c in csts {
        for f in c.nodes(FunctionDecl) {
            if routine_last_name(c, f) != "write" {
                continue;
            }
            let Some(pl) = c.tree.child(f, PortList) else { continue };
            let Some(first) = c
                .tree
                .children(pl)
                .filter(|&p| c.kind(p) == Port)
                .find_map(|p| c.tree.child(p, Declarator).and_then(|d| name_node(c, d)))
            else {
                continue;
            };
            let param = c.tree.bare_text(first);
            let prefix = format!("{param}.");
            let spaced = format!(" {param}.");
            for n in subtree(c, f) {
                let Some((lhs, _)) = assign_parts(c, n) else { continue };
                let text = c.text(lhs);
                if !(text.starts_with(&prefix) || text.contains(&spaced)) {
                    continue;
                }
                let Some(t) = c.first_tok(lhs) else { continue };
                s.add(
                    "UVM-TLM-005",
                    "AnalysisTransactionMutation",
                    Severity::Warning,
                    format!(
                        "Direct in-place modification of analysis transaction '{param}' detected in write() method ('{text}'). \
                         Clone or copy the transaction (e.g. $cast(local_tr, {param}.clone())) before modifying its contents to prevent race conditions across broadcast subscribers."
                    ),
                    c.loc(t),
                );
            }
        }
    }
}
