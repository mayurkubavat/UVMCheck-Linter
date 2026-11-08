//! SV-MET-008 (RandFieldBudget): Flags a sequence item that declares more than
//! the rand-field budget (default 8, option max_rand), counting rand and randc
//! properties of the class itself.

use sv_syntax::SyntaxKind;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::class_index::{Classes, Role, decl_names};

pub(crate) fn rand_field_budget(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    let limit = 8;
    for i in classes.user() {
        if classes.role(i) != Role::Item {
            continue;
        }
        let cls = &classes.list[i];
        let mut n = 0;
        let mut loc = c.loc(cls.tok);
        for item in c.tree.children(cls.node).filter(|&x| c.kind(x) == DataDecl) {
            if !c.toks(item).any(|t| matches!(c.text_of(t), "rand" | "randc")) {
                continue;
            }
            for (_, tok) in decl_names(c, item) {
                n += 1;
                if n == limit + 1 {
                    loc = c.loc(tok);
                }
            }
        }
        if n <= limit {
            continue;
        }
        s.add(
            "SV-MET-008",
            "RandFieldBudget",
            Severity::Warning,
            format!(
                "Sequence item '{}' declares {n} rand fields (limit {limit}). Split the item, or mark fields that the solver should not choose as non-rand.",
                cls.name
            ),
            loc,
        );
    }
}
