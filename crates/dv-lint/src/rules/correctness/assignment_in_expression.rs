//! SV-COR-007 (AssignmentInExpression): Disallows assignment expressions ('='
//! or compound assignments) inside conditional predicates or loop conditions.

use crate::support::for_loops::{for_parts, keyword_is, kids, op_text, predicates, subtree};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn assignment_in_condition(c: &Cst, s: &mut Sink) {
    let mut roots = predicates(c);
    for l in c.nodes(LoopStmt) {
        if keyword_is(c, l, "while") || keyword_is(c, l, "repeat") {
            roots.extend(kids(c, l).first().copied());
        } else if keyword_is(c, l, "for") {
            roots.extend(for_parts(c, l).1);
        }
    }
    for l in c.nodes(DoWhileStmt) {
        let k = kids(c, l);
        if k.len() >= 2 {
            roots.push(k[k.len() - 1]);
        }
    }
    let mut found = Vec::new();
    for r in roots {
        let mut all = Vec::new();
        subtree(c, r, &mut all);
        for n in all {
            if c.kind(n) == BinaryExpr && matches!(op_text(c, n), "=" | "+=" | "-=" | "*=" | "/=") {
                found.push(n);
            }
        }
    }
    found.sort_by_key(|&n| c.first_tok(n));
    for n in found {
        let Some(t) = c.toks(n).next() else { continue };
        s.add(
            "SV-COR-007",
            "AssignmentInExpression",
            Severity::Warning,
            format!(
                "Assignment operator '{}' used inside conditional predicate. In SystemVerilog, an assignment assigns the value and evaluates its truthiness, which is frequently an unintended typo for the equality operator '=='.",
                c.text_of(t)
            ),
            c.loc(t),
        );
    }
}
