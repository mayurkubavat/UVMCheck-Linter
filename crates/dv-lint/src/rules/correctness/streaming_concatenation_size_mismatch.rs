//! SV-STR-001 (StreamingConcatenationSizeMismatch): Disallows streaming
//! concatenation operators ({>> { ... }} / {<< { ... }}) without explicit slice
//! size.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::Child;
use svlang::syntax::SyntaxKind::*;

pub(crate) fn stream_slice(c: &Cst, s: &mut Sink) {
    for n in c.nodes(ConcatExpr) {
        let Some(op) = c.tok(n, "<<").or_else(|| c.tok(n, ">>")) else { continue };
        // A slice size is a node between the operator and the inner `{`.
        let mut after_op = false;
        let mut sliced = false;
        for k in c.kids(n) {
            match *k {
                Child::Token(t) if t == op => after_op = true,
                Child::Token(t) if after_op && c.text_of(t) == "{" => break,
                Child::Node(_) if after_op => sliced = true,
                _ => {}
            }
        }
        if sliced {
            continue;
        }
        let o = c.text_of(op);
        s.add(
            "SV-STR-001",
            "StreamingConcatenationSizeMismatch",
            Severity::Warning,
            format!(
                "Streaming operator '{o}' omits explicit slice size ('{{{o} {{ ... }}}}'). \
                 Per IEEE 1800 § 11.4.14, omitting slice size creates an unbounded single-slice stream across operands, risking silent truncation or endianness misalignments when packing or unpacking composite types and queues. \
                 Specify an explicit slice width (e.g. '{{{o} 8 {{ ... }}}}' or '{{{o} byte {{ ... }}}}')."
            ),
            c.loc(op),
        );
    }
}
