//! SV-LSP-001 (LspConstraintContradiction): Disallows unconditionally disabling
//! inherited constraints via '.constraint_mode(0)' in derived classes.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn lsp_constraint_mode(c: &Cst, s: &mut Sink) {
    for cls in c.nodes(ClassDecl) {
        let Some(ext) = c.tree.child(cls, Header).and_then(|h| c.tree.child(h, Extends)) else { continue };
        let Some(base) = c.tree.children(ext).next() else { continue };
        let class_name = c.tree.child(cls, Name).map(|n| c.tree.bare_text(n)).unwrap_or_default();
        // The base name's last token, without its parameters.
        let base_name = c
            .tree
            .children(base)
            .find(|&k| c.kind(k) == ParamList)
            .and_then(|p| c.first_tok(p))
            .map_or_else(|| c.last_text(base).to_string(), |p| c.text_of(p - 1).to_string());
        for m in c.items(cls).into_iter().filter(|&m| !matches!(c.kind(m), Name | Header)) {
            let text = c.text(m);
            if !text.contains("constraint_mode(0)") && !text.contains("constraint_mode(1'b0)") {
                continue;
            }
            let Some(t) = c.first_tok(m) else { continue };
            s.add(
                "SV-LSP-001",
                "LspConstraintContradiction",
                Severity::Error,
                format!(
                    "Derived class '{class_name}' unconditionally disables an inherited constraint via '.constraint_mode(0)'. \
                     This violates the Liskov Substitution Principle (LSP); components receiving base class '{base_name}' references expect base random contracts to hold. \
                     Refactor base constraints using 'soft' constraints or constraint overriding instead of brute-force disabling."
                ),
                c.loc(t),
            );
        }
    }
}
