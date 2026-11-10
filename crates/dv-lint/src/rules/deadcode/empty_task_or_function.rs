//! SV-DED-011 (EmptyTaskOrFunction): Flags concrete, non-virtual, non-extern
//! tasks and functions with empty bodies (zero statements).

use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn empty_routine(c: &Cst, s: &mut Sink) {
    for n in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)) {
        let Some(name) = c.tree.child(n, Name) else { continue };
        let task = c.kind(n) == TaskDecl;
        if c.tok(n, if task { "endtask" } else { "endfunction" }).is_none() {
            continue;
        }
        let in_class = c.parent(n).is_some_and(|p| c.kind(p) == ClassDecl);
        if in_class && c.toks(n).any(|t| matches!(c.text_of(t), "pure" | "virtual" | "extern")) {
            continue;
        }
        let routine = c.trimmed(name);
        if routine == "body" || routine == "new" {
            continue;
        }
        let has_stmt = c.tree.child(n, Body).is_some_and(|b| c.tree.children(b).any(|x| c.kind(x) != EmptyStmt));
        if has_stmt {
            continue;
        }
        let Some(at) = c.first_tok(name) else { continue };
        s.add(
            "SV-DED-011",
            "EmptyTaskOrFunction",
            Severity::Warning,
            format!(
                "{} '{routine}' has an empty body with zero executable statements. \
                 Concrete routines with empty bodies execute as no-ops, consume simulation call-stack overhead, \
                 and typically represent unfinished testbench scaffolding or forgotten implementation stubs.",
                if task { "Task" } else { "Function" }
            ),
            c.loc(at),
        );
    }
}
