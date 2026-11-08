//! SV-DED-013 (UnusedRoutineArgument): Flags task and function arguments that
//! the body never uses (clang-tidy misc-unused-parameters, Pylint W0613).

use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::handles::{body_refs_if_good, routine_is_virtual};

pub(crate) fn unused_routine_argument(c: &Cst, out: &mut Sink) {
    for (r, n) in crate::support::slang_ast::user_routines(c) {
        let name = crate::support::slang_ast::last_name(c, r);
        if name.ends_with("_phase") || routine_is_virtual(c, r) {
            continue;
        }
        let Some(body) = c.tree.child(r, Body) else { continue };
        let Some(pl) = c.tree.child(r, PortList) else { continue };
        let args: Vec<(NodeId, NodeId)> = c
            .tree
            .children(pl)
            .filter(|&p| c.kind(p) == Port)
            .flat_map(|p| c.tree.children(p).filter(|&d| c.kind(d) == Declarator).collect::<Vec<_>>())
            .filter_map(|d| c.tree.child(d, Name).map(|nm| (d, nm)))
            .collect();
        if args.is_empty() || crate::support::class_index::count_statements(c, body) == 0 {
            continue;
        }
        // A body slang cannot bind has no statements.
        let Some(used) = body_refs_if_good(c, body) else { continue };
        for (d, nm) in args {
            let a = c.tree.bare_text(nm);
            if a.is_empty() || a.starts_with('_') || a.starts_with("unused") || used.contains(&d) {
                continue;
            }
            let Some(loc) = c.first_tok(nm).map(|t| c.loc(t)) else { continue };
            for _ in 0..n {
                out.add(
                    "SV-DED-013",
                    "UnusedRoutineArgument",
                    Severity::Info,
                    format!("Argument '{a}' of '{name}' is never used. Remove it, or use it if the caller expects it to have an effect."),
                    loc,
                );
            }
        }
    }
}
