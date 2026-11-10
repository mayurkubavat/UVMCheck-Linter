//! SVA-CTX-001 (ConcurrentAssertInFunction): Disallow concurrent assertions
//! ('assert/assume/cover property') inside functions.

use crate::support::procedural::start;
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind::*;

pub(crate) fn concurrent_in_function(c: &Cst, s: &mut Sink) {
    for a in c.nodes(ConcurrentAssert) {
        let Some(r) = ancestors(c, a).find(|&x| matches!(c.kind(x), FunctionDecl | TaskDecl)) else { continue };
        let name: String = decl_name(c, r).chars().filter(|ch| !ch.is_whitespace()).collect();
        let Some(loc) = start(c, a) else { continue };
        s.add(
            "SVA-CTX-001",
            "ConcurrentAssertInFunction",
            Severity::Error,
            format!(
                "Illegal concurrent assertion inside function '{name}'. Functions in SystemVerilog (IEEE 1800 § 13.4.4 & § 16.14.1) execute in zero simulation time and cannot participate in clock-domain sampled value checking. Move the assertion to a module, interface, checker, or task."
            ),
            loc,
        );
    }
}
