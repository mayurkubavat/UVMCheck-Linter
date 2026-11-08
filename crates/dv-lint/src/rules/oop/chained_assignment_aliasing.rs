//! SV-OOP-012 (ChainedAssignmentAliasing): Disallows chained assignment
//! expressions (e.g. a = b = c;).

use sv_syntax::NodeId;
use crate::support::class_items::{kids, op};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn chained_assignment(c: &Cst, s: &mut Sink) {
    let is_assign = |n: NodeId| c.kind(n) == BinaryExpr && op(c, n) == "=";
    for n in c.nodes(BinaryExpr) {
        if !is_assign(n) {
            continue;
        }
        let k = kids(c, n);
        if !k.iter().take(2).any(|&x| is_assign(x)) {
            continue;
        }
        let Some(at) = c.toks(n).next() else { continue };
        s.add(
            "SV-OOP-012",
            "ChainedAssignmentAliasing",
            Severity::Warning,
            "Chained assignment expression detected. \
             In SystemVerilog verification environments, chained assignments (e.g. 'a = b = c;') create shared object aliases \
             across multiple references without clone safety, leading to unintentional state mutations across threads. \
             Split chained assignments into discrete statements."
                .into(),
            c.loc(at),
        );
    }
}
