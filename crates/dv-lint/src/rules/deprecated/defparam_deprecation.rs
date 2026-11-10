//! SV-DEP-002 (DefparamDeprecation): Disallow 'defparam' statements.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn defparam(c: &Cst, s: &mut Sink) {
    for n in c.nodes(ParamDecl) {
        let Some(kw) = c.tok(n, "defparam") else { continue };
        s.add(
            "SV-DEP-002",
            "DefparamDeprecation",
            Severity::Error,
            "Hierarchical parameter override 'defparam' is formally deprecated in IEEE 1800 § 23.10.1. \
             defparam violates module encapsulation, introduces cross-hierarchy elaboration dependencies, \
             and prevents simulator caching. Use named parameter overrides at the instantiation site (#(.PARAM(val)))."
                .into(),
            c.loc(kw),
        );
    }
}
