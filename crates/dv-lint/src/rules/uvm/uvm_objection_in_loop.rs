//! UVM-OBJ-002 (UvmObjectionInLoop): Avoid raising and dropping objections
//! inside high-frequency transaction loops to prevent severe simulation
//! performance degradation.

use crate::support::uvm_classes::{call_name, calls_in};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use sv_syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn objection_in_loop(_d: &Design, csts: &[Cst], s: &mut Sink) {
    for c in csts {
        for n in calls_in(c, c.tree.root) {
            let name = call_name(c, n);
            if name != "raise_objection" && name != "drop_objection" {
                continue;
            }
            let mut in_loop = false;
            let mut cur = c.parent(n);
            while let Some(p) = cur {
                match c.kind(p) {
                    LoopStmt | DoWhileStmt => {
                        in_loop = true;
                        break;
                    }
                    FunctionDecl | TaskDecl | ClassDecl => break,
                    _ => {}
                }
                cur = c.parent(p);
            }
            if !in_loop {
                continue;
            }
            let Some(t) = c.first_tok(n) else { continue };
            s.add(
                "UVM-OBJ-002",
                "UvmObjectionInLoop",
                Severity::Warning,
                format!(
                    "UVM objection method '{name}' invoked inside a loop. Raising or dropping objections per iteration \
                     causes severe simulation performance degradation. Raise the objection once before the loop and drop it after the loop completes."
                ),
                c.loc(t),
            );
        }
    }
}
