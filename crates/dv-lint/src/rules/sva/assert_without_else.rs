//! SVA-ACT-001 (AssertWithoutElse): Immediate and concurrent assertions must
//! include an 'else' action block (e.g. 'else `uvm_error(...)').

use crate::support::procedural::start;
use crate::support::slang_ast::*;
use crate::support::sva_cov::{add_n, assert_kw, ast_asserts};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

fn has_else(c: &Cst, a: NodeId) -> bool {
    c.tree.child(a, ElseClause).is_some()
}

pub(crate) fn assert_without_else(c: &Cst, s: &mut Sink) {
    for a in ast_asserts(c, AssertStmt) {
        if assert_kw(c, a) != "assert" || has_else(c, a) {
            continue;
        }
        if let Some(loc) = start(c, a) {
            add_n(
                s,
                times(c, a),
                "SVA-ACT-001",
                "AssertWithoutElse",
                Severity::Warning,
                "Immediate assertion lacks an 'else' action block. Assertions without explicit action blocks \
                 (e.g. 'else `uvm_error(\"ASSERT_FAIL\", \"...\")') print generic default messages and bypass UVM report catchers."
                    .into(),
                loc,
            );
        }
    }
    for a in ast_asserts(c, ConcurrentAssert) {
        if assert_kw(c, a) != "assert" || has_else(c, a) {
            continue;
        }
        if let Some(loc) = start(c, a) {
            add_n(
                s,
                times(c, a),
                "SVA-ACT-001",
                "AssertWithoutElse",
                Severity::Warning,
                "Concurrent assertion lacks an 'else' action block. When this assertion fails, the simulator emits \
                 a generic one-line diagnostic without severity filtering or payload context. Add 'else `uvm_error(...)'."
                    .into(),
                loc,
            );
        }
    }
}
