//! SV-THR-002 (StaticRoutineVariableLifetime): a task outside a class has
//! static lifetime by default, so its locals are shared by every call that
//! is running. Two calls that overlap in time overwrite each other's values.
//!
//! Only tasks are reported, as a warning. A function returns in zero time,
//! so its calls cannot overlap and a static local is harmless there (it was
//! 34 of the 36 findings in Accellera's UVM library, every one a false
//! alarm). A task's static local is a hazard, not a certain bug: a counter
//! kept across calls on purpose looks the same, which is why this is not an
//! error.

use crate::support::procedural::{add_n, routine_name};
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn static_routine_variable(c: &Cst, s: &mut Sink) {
    for k in [TaskDecl] {
        for r in c.nodes(k) {
            if enclosing_class(c, r).is_some() {
                continue;
            }
            if has_kw(c, r, "automatic") {
                continue;
            }
            // The routine's default lifetime is its unit's.
            let unit = ancestors(c, r)
                .find(|&a| matches!(c.kind(a), ModuleDecl | InterfaceDecl | ProgramDecl | PackageDecl | CheckerDecl));
            if unit.is_some_and(|u| has_kw(c, u, "automatic")) {
                continue;
            }
            let Some(body) = c.tree.child(r, Body) else { continue };
            let n = times(c, r);
            let rname = routine_name(c, r);
            for d in kids(c, body) {
                if c.kind(d) != DataDecl {
                    continue;
                }
                let toks = tok_texts(c, d);
                if toks
                    .iter()
                    .any(|t| matches!(*t, "automatic" | "const" | "typedef" | "input" | "output" | "inout" | "ref"))
                {
                    continue;
                }
                if c.tree.child(d, DataType).is_some_and(|t| tok_texts(c, t).first() == Some(&"const")) {
                    continue;
                }
                for decl in c.tree.children(d).filter(|&x| c.kind(x) == Declarator) {
                    let Some(nm) = c.tree.child(decl, Name) else { continue };
                    let Some(t) = c.first_tok(nm) else { continue };
                    let vname = c.text_of(t);
                    add_n(
                        s,
                        n,
                        "SV-THR-002",
                        "StaticRoutineVariableLifetime",
                        Severity::Warning,
                        format!(
                            "Local variable '{vname}' in task '{rname}' is static: a task outside a class has static \
                             lifetime by default, so calls that overlap in time share '{vname}'. Declare the task \
                             'automatic'."
                        ),
                        c.loc(t),
                    );
                }
            }
        }
    }
}
