//! SV-DED-007 (UnusedPureRoutineResult): Flags calls to pure functions or
//! value-returning routines ($sformatf, $realtime, len, toupper, sum) used as
//! standalone statements where the result is discarded.

use crate::support::statements::exprs;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn unused_pure_result(c: &Cst, s: &mut Sink) {
    for st in c.nodes(ExprStmt) {
        let Some(call) = exprs(c, st).into_iter().next() else {
            continue;
        };
        if c.kind(call) != CallExpr {
            continue;
        }
        let Some(callee) = c.tree.children(call).next() else {
            continue;
        };
        let Some((first, last)) = c.tree.token_bounds(callee) else {
            continue;
        };
        let name = c.text_of(last);
        if name == "$sformatf" {
            s.add(
                "SV-DED-007",
                "UnusedPureRoutineResult",
                Severity::Warning,
                "Result of pure system function '$sformatf()' is discarded in a standalone statement. \
                 In SystemVerilog, $sformatf() returns a formatted string without printing or modifying arguments. \
                 Use $display() to output to the console or $sformat(dest, ...) to write to a string variable."
                    .into(),
                c.loc(first),
            );
        } else if matches!(
            name,
            "$realtime"
                | "$time"
                | "$stime"
                | "$clog2"
                | "$bits"
                | "$typename"
                | "$sin"
                | "$cos"
                | "$tan"
                | "$ln"
                | "$log10"
                | "$exp"
                | "$sqrt"
                | "$pow"
        ) {
            s.add(
                "SV-DED-007",
                "UnusedPureRoutineResult",
                Severity::Warning,
                format!("Result of value-returning system function '{name}()' is discarded in a standalone statement."),
                c.loc(first),
            );
        } else if matches!(
            name,
            "len"
                | "substr"
                | "toupper"
                | "tolower"
                | "compare"
                | "icompare"
                | "atoi"
                | "atohex"
                | "atobin"
                | "atooct"
                | "atoreal"
                | "sum"
                | "product"
        ) {
            s.add(
                "SV-DED-007",
                "UnusedPureRoutineResult",
                Severity::Warning,
                format!("Result of value-returning pure routine '.{name}()' is discarded in a standalone statement."),
                c.loc(last),
            );
        }
    }
}
