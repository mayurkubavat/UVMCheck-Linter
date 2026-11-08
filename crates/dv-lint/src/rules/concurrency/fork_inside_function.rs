//! SV-FOR-001 (ForkInsideFunction): Disallows spawning concurrent threads with
//! 'fork ... join/join_any/join_none' inside functions.

use sv_syntax::SyntaxKind;
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::procedural::{join_kind, routine_name};

pub(crate) fn fork_in_function(c: &Cst, s: &mut Sink) {
    for f in c.nodes(FunctionDecl) {
        let name = routine_name(c, f);
        for b in subtree(c, f) {
            if c.kind(b) != ParBlock {
                continue;
            }
            let Some(kw) = c.tok(b, "fork") else { continue };
            let join = join_kind(c, b);
            s.add(
                "SV-FOR-001",
                "ForkInsideFunction",
                Severity::Warning,
                format!(
                    "Spawning concurrent background process with 'fork ... {join}' inside function '{name}()'. \
                     IEEE 1800 § 13.4.4 requires functions to remain non-time-consuming and instantaneous; \
                     spawning threads from functions creates untracked background execution and risks corrupting local automatic variables after the function returns. \
                     Refactor concurrent thread creation to a task."
                ),
                c.loc(kw),
            );
        }
    }
}
