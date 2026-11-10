//! SV-PRF-003 (DynArrayIncrementalResizeInLoop): Growing dynamic arrays
//! incrementally inside loops with new[arr.size()+1](arr) causes O(N^2) memory
//! reallocation bottlenecks.

use crate::support::ast::Ast;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use svlang::syntax::SyntaxKind;
use SyntaxKind::*;

fn resize_in_loop(c: &Cst, a: &Ast, s: &mut Sink) {
    for n in c.nodes(NewExpr) {
        // `new [n](init)`: the size, then the array to copy.
        if c.tok(n, "[").is_none() || c.tok(n, "(").is_none() {
            continue;
        }
        let mut cur = c.parent(n);
        let mut in_loop = false;
        while let Some(p) = cur {
            let kw = c.keyword(p).map(|t| c.text_of(t)).unwrap_or("");
            if (c.kind(p) == LoopStmt && matches!(kw, "for" | "foreach" | "while")) || c.kind(p) == DoWhileStmt {
                in_loop = true;
                break;
            }
            cur = c.parent(p);
        }
        if !in_loop {
            continue;
        }
        let Some(at) = c.first_tok(n) else { continue };
        s.add(
            "SV-PRF-003",
            "DynArrayIncrementalResizeInLoop",
            Severity::Warning,
            "Dynamic array resize 'new[...](existing_array)' invoked inside a loop. \
             Incrementally resizing dynamic arrays creates an O(N^2) heap reallocation and element-copying bottleneck. \
             Pre-allocate the full array with 'new[size]' before the loop, or use a SystemVerilog queue ([$])."
                .into(),
            c.loc(at),
        );
        a.again(s, c, n);
    }
}

pub(crate) fn ast_resize_in_loop(d: &Design, csts: &[Cst], s: &mut Sink) {
    for c in csts {
        resize_in_loop(c, &Ast::new(d, c), s);
    }
}
