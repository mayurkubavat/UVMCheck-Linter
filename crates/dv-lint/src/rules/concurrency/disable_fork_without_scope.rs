//! SV-CON-010 (DisableForkWithoutScope): Disallows calling 'disable fork;'
//! directly at task or routine scope without an enclosing isolating 'fork ...
//! join' block.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind::*;

pub(crate) fn disable_fork(c: &Cst, s: &mut Sink) {
    for n in c.nodes(DisableStmt) {
        if c.tok(n, "fork").is_none() {
            continue;
        }
        let mut enclosed = false;
        let mut cur = c.parent(n);
        while let Some(p) = cur {
            match c.kind(p) {
                FunctionDecl | TaskDecl | ModuleDecl | ClassDecl => break,
                ParBlock => {
                    enclosed = true;
                    break;
                }
                _ => cur = c.parent(p),
            }
        }
        if enclosed {
            continue;
        }
        let Some(kw) = c.tok(n, "disable") else { continue };
        s.add(
            "SV-CON-010",
            "DisableForkWithoutScope",
            Severity::Error,
            "Unenclosed 'disable fork;' executed directly at routine scope terminates all active child processes of the calling thread. \
             In IEEE 1800 § 9.6.3 concurrency guidelines, 'disable fork;' must be enclosed inside an isolating \
             'fork begin ... disable fork; end join' block to avoid inadvertently killing sibling or parent threads."
                .into(),
            c.loc(kw),
        );
    }
}
