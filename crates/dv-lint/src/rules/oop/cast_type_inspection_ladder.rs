//! SV-OCP-001 (CastTypeInspectionLadder): Disallows cascading $cast ladders (3+
//! branches) inspecting object types.

use crate::support::calls::kids;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind::*;

pub(crate) fn cast_ladder(c: &Cst, s: &mut Sink) {
    for n in c.nodes(IfStmt) {
        if c.parent(n).is_some_and(|p| c.kind(p) == ElseClause) {
            continue;
        }
        let mut casts = 0;
        let mut cur = Some(n);
        while let Some(i) = cur {
            if kids(c, i).first().is_some_and(|&p| c.text(p).contains("$cast")) {
                casts += 1;
            }
            cur = c.tree.child(i, ElseClause).and_then(|e| c.tree.children(e).next()).filter(|&x| c.kind(x) == IfStmt);
        }
        if casts < 3 {
            continue;
        }
        let Some(t) = c.tok(n, "if") else { continue };
        s.add(
            "SV-OCP-001",
            "CastTypeInspectionLadder",
            Severity::Warning,
            format!(
                "Cascading dynamic cast ladder ($cast) detected with {casts} branches. \
                 Type inspection ladders violate the Open-Closed Principle (OCP) and bypass polymorphic dispatch. \
                 Introducing new subclasses requires modifying each ladder across the testbench. Refactor common branch operations into virtual methods on the base class."
            ),
            c.loc(t),
        );
    }
}
