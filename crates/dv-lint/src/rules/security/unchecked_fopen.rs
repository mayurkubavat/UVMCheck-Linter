//! SV-SYS-001 (UncheckedFopen): Flags unchecked $fopen return descriptors and
//! discarded $fopen calls.

use sv_syntax::SyntaxKind;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::ast::{kids, subtree};

pub(crate) fn unchecked_fopen(c: &Cst, s: &mut Sink) {
    for n in c.nodes(CallExpr) {
        let Some(&callee) = kids(c, n).first() else { continue };
        if c.trimmed(callee) != "$fopen" {
            continue;
        }
        let Some(at) = c.first_tok(n) else { continue };
        let Some(p) = c.parent(n) else { continue };
        if c.kind(p) == ExprStmt {
            s.add(
                "SV-SYS-001",
                "UncheckedFopen",
                Severity::Warning,
                "Return descriptor of '$fopen' is discarded. In SystemVerilog (IEEE 1800 § 21.3.1), '$fopen' returns a 32-bit descriptor; \
                 discarding the return value leaks file handles and prevents error validation."
                    .into(),
                c.loc(at),
            );
            continue;
        }
        // `fd = $fopen(...)`: slang's AssignmentExpression.
        if c.kind(p) != BinaryExpr || c.toks(p).next().map(|t| c.text_of(t)) != Some("=") {
            continue;
        }
        let Some(&lhs) = kids(c, p).first() else { continue };
        if lhs == n {
            continue;
        }
        let var = c.last_text(lhs).to_string();
        let mut enclosing = c.parent(p);
        while let Some(e) = enclosing {
            if matches!(c.kind(e), FunctionDecl | TaskDecl | SeqBlock | ModuleDecl) {
                break;
            }
            enclosing = c.parent(e);
        }
        let Some(e) = enclosing else { continue };
        let checked = subtree(c, e).into_iter().any(|x| {
            c.kind(x) == IfStmt && kids(c, x).first().is_some_and(|&cond| c.text(cond).contains(var.as_str()))
        });
        if checked {
            continue;
        }
        s.add(
            "SV-SYS-001",
            "UncheckedFopen",
            Severity::Warning,
            format!(
                "File descriptor '{var}' returned by '$fopen' is not verified against 0/NULL (e.g. 'if ({var} == 0)'). \
                 If the file cannot be opened, subsequent writes will crash the simulator or result in lost log data."
            ),
            c.loc(at),
        );
    }
}
