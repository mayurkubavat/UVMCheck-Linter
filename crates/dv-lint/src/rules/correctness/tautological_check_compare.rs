//! SV-COR-001 (TautologicalCheckCompare): Detects tautological comparison
//! between identical expressions (e.g. 'act_pkt.data == act_pkt.data').

use crate::support::class_items::{kids, op, trim};
use crate::support::data_items::{is_number_literal, strip_conv, unparen};
use crate::support::sema_facts::{ast_ok, visits};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn tautological_compare(c: &Cst, s: &mut Sink) {
    for n in c.nodes(BinaryExpr) {
        if !matches!(op(c, n), "==" | "!=" | "===" | "!==" | "==?" | "!=?" | "<" | "<=" | ">" | ">=") {
            continue;
        }
        // `<=` as a statement's operator is a nonblocking assignment.
        if op(c, n) == "<=" && c.parent(n).is_some_and(|p| matches!(c.kind(p), ExprStmt)) {
            continue;
        }
        let k = kids(c, n);
        let (Some(&l), Some(&r)) = (k.first(), k.get(1)) else { continue };
        let ((l, ls), (r, rs)) = (strip_conv(c, l), strip_conv(c, r));
        if is_number_literal(c, unparen(c, l)) || is_number_literal(c, unparen(c, r)) {
            continue;
        }
        let (ls, rs) = (trim(&ls).to_string(), trim(&rs).to_string());
        if ls.is_empty() || ls != rs || !ls.chars().any(|x| x.is_ascii_alphabetic() || x == '_') {
            continue;
        }
        if !ast_ok(c, n) {
            continue;
        }
        let Some(at) = c.first_tok(n) else { continue };
        for _ in 0..visits(c, n) {
            s.add(
                "SV-COR-001",
                "TautologicalCheckCompare",
                Severity::Error,
                format!(
                    "Tautological comparison between identical expressions '{ls}' and '{rs}'. \
                     In verification checks and scoreboards, comparing an expression to itself always evaluates to a constant \
                     result, masking functional RTL bugs and creating silent false-positive test results."
                ),
                c.loc(at),
            );
        }
    }
}
