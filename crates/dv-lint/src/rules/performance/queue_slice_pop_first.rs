//! SV-PRF-008 (QueueSlicePopFirst): Disallow queue slicing ('q = q[1:$]') to
//! drop head elements.

use crate::support::ast::{assignment_op, kids, source_text, symbol_ref, type_head, typedef_named, unparen, Ast, Decl};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use sv_syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

pub(crate) fn ast_queue_slice(d: &Design, csts: &[Cst], s: &mut Sink) {
    for c in csts {
        queue_slice(c, &Ast::new(d, c), s);
    }
}

/// The declared type is a queue (`T q[$]`, or a typedef of one).
fn is_queue(c: &Cst, d: &Decl) -> bool {
    let first_dim = |x: NodeId| c.tree.child(x, Dim).map(|dim| c.tree.bare_text(dim));
    if c.kind(d.item) != Declarator {
        return false;
    }
    if let Some(dim) = first_dim(d.item) {
        return dim == "[$]" || dim.starts_with("[$:");
    }
    let Some(dt) = d.dtype else { return false };
    let Some(name) = type_head(c, dt) else { return false };
    typedef_named(c, name).and_then(first_dim).is_some_and(|dim| dim == "[$]" || dim.starts_with("[$:"))
}

fn queue_slice(c: &Cst, a: &Ast, s: &mut Sink) {
    for n in c.nodes(BinaryExpr) {
        if assignment_op(c, n) != "=" {
            continue;
        }
        let k = kids(c, n);
        let [lhs, rhs] = k[..] else { continue };
        // Cheap shape test first: symbol lookups are costly.
        let rhs = unparen(c, rhs);
        if c.kind(rhs) != SelectExpr || c.tok(rhs, ":").is_none() {
            continue;
        }
        let text = source_text(c, rhs);
        if !(text.contains("1:$") || text.contains("1 : $")) {
            continue;
        }
        let Some((d, name)) = symbol_ref(c, lhs) else { continue };
        if !is_queue(c, &d) {
            continue;
        }
        let Some(&value) = kids(c, rhs).first() else { continue };
        if symbol_ref(c, value).map(|x| x.0) != Some(d) {
            continue;
        }
        let Some(at) = c.first_tok(lhs) else { continue };
        s.add(
            "SV-PRF-008",
            "QueueSlicePopFirst",
            Severity::Warning,
            format!(
                "Queue slicing '{name} = {name}[1:$]' duplicates the entire remaining queue into newly allocated memory. \
                 Use in-place 'void'({name}.pop_front())' or 'item = {name}.pop_front()' for O(1) pointer adjustment."
            ),
            c.loc(at),
        )
        .fixes
        .extend(c.tree.token_bounds(n).and_then(|(x, y)| {
            c.token_fix(x, y, format!("void'({name}.pop_front())"), &format!("Replace queue slice with void'({name}.pop_front())"))
        }));
        a.again(s, c, n);
    }
}
