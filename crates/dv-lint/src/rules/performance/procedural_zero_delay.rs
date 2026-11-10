//! SV-PRF-004 (ProceduralZeroDelay): Procedural delay #0 reschedules execution
//! into the Inactive region.

use crate::support::ast::{int_value, kids, source_text, unparen, Ast};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use svlang::syntax::SyntaxKind;
use SyntaxKind::*;

fn zero_delay(c: &Cst, a: &Ast, s: &mut Sink) {
    for n in c.nodes(TimedStmt) {
        let Some(tc) = c.tree.child(n, TimingControl) else { continue };
        let Some(hash) = c.tok(tc, "#") else { continue };
        let Some(expr) = kids(c, tc).first().copied() else { continue };
        let expr = unparen(c, expr);
        let zero = int_value(c, expr) == Some(0) || source_text(c, expr) == "0";
        if !zero {
            continue;
        }
        let at = c.first_tok(n).unwrap_or(hash);
        s.add(
            "SV-PRF-004",
            "ProceduralZeroDelay",
            Severity::Warning,
            "Procedural delay '#0' reschedules the process into the Inactive simulation region. \
             Cascades of #0 delays cause delta-cycle thrashing, inflate event engine queues, and mask underlying race conditions. \
             Use non-blocking assignments (<=), clocking block skews, or explicit event synchronization instead."
                .into(),
            c.loc(at),
        );
        a.again(s, c, n);
    }
}

pub(crate) fn ast_zero_delay(d: &Design, csts: &[Cst], s: &mut Sink) {
    for c in csts {
        zero_delay(c, &Ast::new(d, c), s);
    }
}
