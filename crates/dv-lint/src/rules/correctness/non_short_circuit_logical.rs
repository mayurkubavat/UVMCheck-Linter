//! SV-COR-005 (NonShortCircuitLogical): Disallows using bitwise AND ('&') or
//! bitwise OR ('|') instead of logical short-circuiting operators ('&&', '||')
//! inside conditional statements.

use crate::support::for_loops::{kids, op_text, predicates};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

const BOOLEAN_OPS: &[&str] = &["==", "!=", "===", "!==", "==?", "!=?", "<", "<=", ">", ">=", "&&", "||", "->", "<->"];

fn is_boolean(c: &Cst, n: NodeId) -> bool {
    c.kind(n) == BinaryExpr && BOOLEAN_OPS.contains(&op_text(c, n))
}

pub(crate) fn non_short_circuit(c: &Cst, s: &mut Sink) {
    fn check(c: &Cst, e: NodeId, s: &mut Sink) {
        match c.kind(e) {
            BinaryExpr if matches!(op_text(c, e), "&" | "|") => {
                let k = kids(c, e);
                let boolean = k.iter().any(|&x| {
                    is_boolean(c, x)
                        || (c.kind(x) == ParenExpr && kids(c, x).first().is_some_and(|&i| is_boolean(c, i)))
                });
                if boolean {
                    let op = op_text(c, e);
                    let rep = if op == "&" { "&&" } else { "||" };
                    if let Some(t) = c.toks(e).next() {
                        s.add(
                            "SV-COR-005",
                            "NonShortCircuitLogical",
                            Severity::Warning,
                            format!(
                                "Bitwise operator '{op}' used in conditional predicate where logical '{rep}' is expected. Bitwise operators do not short-circuit, unconditionally evaluating the right-hand side and risking null-pointer dereferences or index-out-of-bounds faults. Replace with '{rep}'."
                            ),
                            c.loc(t),
                        );
                    }
                }
                for x in k {
                    check(c, x, s);
                }
            }
            ParenExpr => {
                if let Some(&i) = kids(c, e).first() {
                    check(c, i, s);
                }
            }
            _ => {}
        }
    }
    for p in predicates(c) {
        check(c, p, s);
    }
}
