//! UVM-PHS-010 (BuildPhaseCrossComponentAccess): Detects direct access to
//! sibling components or cross-component handles during build_phase.

use crate::support::uvm_classes::is_name_chain;
use crate::support::uvm_syntax::{routine_last_name, subtree};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use sv_syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

pub(crate) fn build_phase_cross_access(_d: &Design, csts: &[Cst], s: &mut Sink) {
    for c in csts {
        for f in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)) {
            if routine_last_name(c, f) != "build_phase" {
                continue;
            }
            for n in subtree(c, f) {
                if c.kind(n) != MemberExpr || !member_access_has_syntax(c, n) {
                    continue;
                }
                let text = c.text(n);
                if !(text.contains("m_parent.") || text.contains("parent.")) {
                    continue;
                }
                let Some(first) = text.find('.') else { continue };
                if !text[first + 1..].contains('.') {
                    continue;
                }
                let Some(t) = c.first_tok(n) else { continue };
                s.add(
                    "UVM-PHS-010",
                    "BuildPhaseCrossComponentAccess",
                    Severity::Warning,
                    format!(
                        "Cross-component access '{text}' detected during build_phase. \
                         UVM does not specify the execution order of build_phase across sibling components. \
                         Move cross-component references to connect_phase or pass configurations using uvm_config_db."
                    ),
                    c.loc(t),
                );
            }
        }
    }
}

/// A member access slang keeps syntax for: the outermost part of a dotted
/// name, or `f().x` (not a method being called).
fn member_access_has_syntax(c: &Cst, n: NodeId) -> bool {
    let parent = c.parent(n);
    let called = parent.is_some_and(|p| c.kind(p) == CallExpr && c.tree.children(p).next() == Some(n));
    if called {
        return false;
    }
    if !is_name_chain(c, n) {
        return true;
    }
    !parent.is_some_and(|p| matches!(c.kind(p), MemberExpr | SelectExpr) && c.tree.children(p).next() == Some(n))
}
