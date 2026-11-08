//! SV-PRF-010 (StringConcatInLoop): Disallows iterative in-place string
//! concatenation or $sformatf recursion inside loop bodies to prevent O(N^2)
//! memory reallocation overhead.

use sv_syntax::NodeId;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;
use crate::support::for_loops::{kids, op_text, subtree};

pub(crate) fn concat_in_loop(c: &Cst, s: &mut Sink) {
    let mut loops = c.nodes(LoopStmt);
    loops.extend(c.nodes(DoWhileStmt));
    loops.sort_by_key(|&n| c.first_tok(n));
    for l in loops {
        let body = match c.keyword(l).map(|k| c.text_of(k)) {
            Some("for" | "foreach" | "while" | "repeat") => kids(c, l).last().copied(),
            Some("do") => kids(c, l).first().copied(),
            _ => None,
        };
        let Some(body) = body else { continue };
        let mut all = Vec::new();
        subtree(c, body, &mut all);
        for n in all {
            if c.kind(n) != BinaryExpr || op_text(c, n) != "=" {
                continue;
            }
            let k = kids(c, n);
            let [left, right] = k[..] else { continue };
            let var = c.last_text(left);
            let Some(op) = c.toks(n).next() else { continue };
            let msg = match c.kind(right) {
                ConcatExpr if is_plain_concat(c, right) && c.text(right).contains(var) => format!(
                    "Iterative concatenation of variable '{var}' inside a loop construct. In SystemVerilog, repeatedly concatenating strings or arrays inside loops causes quadratic heap allocations and memory copies (O(N^2)). Accumulate elements into a queue ('q.push_back(...)') or pre-allocate fixed buffers instead."
                ),
                CallExpr
                    if kids(c, right).first().is_some_and(|&f| c.last_text(f) == "$sformatf")
                        && c.text(right).contains(var) =>
                {
                    format!(
                        "Iterative string formatting into '{var}' via $sformatf inside a loop construct. Repeatedly reallocating and copying strings inside loops causes severe memory fragmentation and simulation slowdown. Accumulate messages into a queue or use stream buffers instead."
                    )
                }
                _ => continue,
            };
            s.add("SV-PRF-010", "StringConcatInLoop", Severity::Warning, msg, c.loc(op));
        }
    }
}

/// slang's `ConcatenationExpression`: not `{}`, a replication or a stream.
fn is_plain_concat(c: &Cst, n: NodeId) -> bool {
    let k = kids(c, n);
    let replication = k.len() == 2 && c.tok(n, ",").is_none() && c.kind(k[1]) == ConcatExpr;
    !k.is_empty() && !replication && c.tok(n, "<<").is_none() && c.tok(n, ">>").is_none()
}
