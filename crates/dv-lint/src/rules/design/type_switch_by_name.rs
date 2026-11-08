//! SV-DSN-001 (TypeSwitchByName): Flags branching on get_type_name() compared
//! with string literals instead of virtual dispatch (Sonar 'instanceof chains',
//! open/closed principle).

use sv_syntax::{NodeId, SyntaxKind};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::statements::{St, Units, call_name, case_parts, op, report, start, stmt, string_literal, strip_parens};

pub(crate) fn type_switch_by_name(c: &Cst, s: &mut Sink) {
    let units = Units::new(c);
    let classes = crate::support::class_index::Classes::new(c);
    let is_name_call =
        |e: NodeId| call_name(c, strip_parens(c, e)) == Some("get_type_name") && c.kind(strip_parens(c, e)) == CallExpr;
    let is_str = |e: NodeId| string_literal(c, strip_parens(c, e)).is_some();
    let msg = "Behavior is chosen by comparing get_type_name() with a string. Every new type then needs \
               edits here, and a renamed class silently falls through. Move the behavior into a virtual \
               method of the class hierarchy (open/closed principle).";
    let mut found: Vec<NodeId> = Vec::new();
    for n in c.nodes(BinaryExpr) {
        if !matches!(op(c, n), "==" | "!=" | "===" | "!==") {
            continue;
        }
        let mut it = c.tree.children(n);
        let (Some(l), Some(r)) = (it.next(), it.next()) else {
            continue;
        };
        if (is_name_call(l) && is_str(r)) || (is_name_call(r) && is_str(l)) {
            found.push(n);
        }
    }
    for n in c.nodes(CaseStmt) {
        if stmt(c, n) != Some(St::Case) {
            continue;
        }
        if let Some(sel) = case_parts(c, n).sel {
            if is_name_call(sel) {
                found.push(sel);
            }
        }
    }
    for n in found {
        if classes.in_library(c, n) {
            continue;
        }
        report(
            s,
            units.unit_times(c, n),
            "SV-DSN-001",
            "TypeSwitchByName",
            Severity::Warning,
            msg.into(),
            start(c, n),
        );
    }
}
