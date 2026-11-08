//! SV-CON-004 (ConstantWaitCondition): Flags 'wait()' statements with
//! compile-time constant conditions (e.g., 'wait(0);', 'wait(1);'). 'wait(0);'
//! deadlocks the simulation thread permanently, while 'wait(1);' executes
//! instantaneously.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

/// slang's IntegerVectorExpression (`4'b1`, `'hF`) or IntegerLiteralExpression (`7`).
fn integer_literal(c: &Cst, n: NodeId) -> bool {
    if c.kind(n) != Literal {
        return false;
    }
    let toks: Vec<&str> = c.toks(n).map(|t| c.text_of(t)).collect();
    let based = |t: &str| {
        let b = t.strip_prefix('\'').map(|r| r.trim_start_matches(['s', 'S']));
        b.is_some_and(|r| r.starts_with(['b', 'B', 'o', 'O', 'd', 'D', 'h', 'H']))
    };
    match toks.as_slice() {
        [t] => {
            t.chars().next().is_some_and(|ch| ch.is_ascii_digit())
                && t.chars().all(|ch| ch.is_ascii_digit() || ch == '_')
                || based(t)
        }
        _ => toks.iter().any(|t| based(t)),
    }
}

pub(crate) fn constant_wait(c: &Cst, s: &mut Sink) {
    for n in c.nodes(WaitStmt) {
        if !c.toks(n).nth(1).is_some_and(|t| c.text_of(t) == "(") {
            continue;
        }
        let Some(e) = c.items(n).into_iter().next() else { continue };
        let expr = c.trimmed(e);
        let constant = matches!(
            expr.as_str(),
            "0" | "1" | "1'b0" | "1'b1" | "1'd0" | "1'd1" | "32'd0" | "32'd1" | "true" | "false"
        ) || integer_literal(c, e);
        if !constant {
            continue;
        }
        let Some(first) = c.first_tok(e) else { continue };
        s.add(
            "SV-CON-004",
            "ConstantWaitCondition",
            Severity::Error,
            format!(
                "Procedural 'wait({expr})' specifies a compile-time constant condition. \
                 'wait(0)' deadlocks the execution thread permanently; 'wait(1)' evaluates instantaneously without synchronization. \
                 Use a dynamic signal expression or explicit event control ('@(event)')."
            ),
            c.loc(first),
        );
    }
}
