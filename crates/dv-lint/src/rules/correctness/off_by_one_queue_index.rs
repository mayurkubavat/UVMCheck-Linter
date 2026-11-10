//! SV-ARR-001 (OffByOneQueueIndex): Detects out-of-bounds off-by-one queue and
//! dynamic array indexing (e.g. 'q[q.size()]').

use crate::support::spans::span;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind::*;

pub(crate) fn off_by_one_index(c: &Cst, s: &mut Sink) {
    for n in c.nodes(SelectExpr) {
        let toks: Vec<u32> = c.toks(n).collect();
        let mut open = None;
        for &t in &toks {
            match c.text_of(t) {
                "[" => open = Some(t),
                "]" => {
                    let Some(o) = open.take() else { continue };
                    let text = span(c, o, t);
                    let text = text.trim_matches([' ', '\t', '\r', '\n']);
                    if text.contains(".size") && !text.contains('-') && !text.contains('+') {
                        s.add(
                            "SV-ARR-001",
                            "OffByOneQueueIndex",
                            Severity::Error,
                            format!(
                                "Out-of-bounds array access '{text}'. \
                                 In SystemVerilog (IEEE 1800 § 7.10.1), queue and dynamic array indices span from 0 to size()-1. \
                                 Accessing index size() is an off-by-one error that silently returns an uninitialized value ('x or 0) \
                                 without triggering a simulator runtime error. Did you mean index [size() - 1]?"
                            ),
                            c.loc(o),
                        );
                    }
                }
                _ => {}
            }
        }
    }
}
