//! SVA-RST-001 (DisableIffReset): Concurrent assertions and property
//! declarations must include a 'disable iff (!rst_n)' clause to gate checking
//! during power-on reset and warm resets, avoiding false-failure storms.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

fn has_disable(c: &Cst, n: NodeId) -> bool {
    c.tree.child(n, PropertyExpr).is_some_and(|p| c.tree.child(p, DisableIff).is_some())
}

pub(crate) fn disable_iff_reset(c: &Cst, s: &mut Sink) {
    for n in c.nodes(PropertyDecl) {
        let Some(name) = c.tree.child(n, Name).and_then(|nm| c.first_tok(nm)) else { continue };
        if c.tree.child(n, PropertyExpr).is_none() || has_disable(c, n) {
            continue;
        }
        s.add(
            "SVA-RST-001",
            "DisableIffReset",
            Severity::Warning,
            format!(
                "Property declaration '{}' lacks a 'disable iff (!rst_n)' reset gating condition. \
                 Concurrent assertions without reset gating evaluate during power-on and warm reset cycles, \
                 causing spurious false-failure storms in simulation regressions.",
                c.text_of(name)
            ),
            c.loc(name),
        );
    }
    for n in c.nodes(ConcurrentAssert) {
        let Some(kw) = c.keyword(n) else { continue };
        let k = c.text_of(kw);
        if !matches!(k, "assert" | "assume") || c.tree.child(n, PropertyExpr).is_none() || has_disable(c, n) {
            continue;
        }
        s.add(
            "SVA-RST-001",
            "DisableIffReset",
            Severity::Warning,
            format!(
                "Inline concurrent assertion '{k} property' lacks a 'disable iff (!rst_n)' reset gating condition. \
                 Add a disable iff condition to prevent spurious assertion violations while the DUT is under reset."
            ),
            c.loc(kw),
        );
    }
}
