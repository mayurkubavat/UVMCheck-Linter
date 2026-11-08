//! SV-TYP-010 (TaskWithNonVoidReturnType): Disallows declaring tasks with
//! explicit non-void return types.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn task_return_type(c: &Cst, s: &mut Sink) {
    for n in c.nodes(TaskDecl) {
        let Some(rt) = c.tree.child(n, ReturnType) else { continue };
        let ret = c.trimmed(rt);
        if ret.is_empty() || ret == "void" {
            continue;
        }
        let Some(first) = c.first_tok(rt) else { continue };
        let name = c.tree.child(n, Name).map(|x| c.trimmed(x)).unwrap_or_else(|| "task".into());
        s.add(
            "SV-TYP-010",
            "TaskWithNonVoidReturnType",
            Severity::Error,
            format!(
                "Task '{name}' is declared with non-void return type '{ret}'. In SystemVerilog (IEEE 1800 § 13.3), tasks cannot return values \
                 and cannot specify a return type. Use a 'function' if a return value is required, or remove the return type."
            ),
            c.loc(first),
        );
    }
}
