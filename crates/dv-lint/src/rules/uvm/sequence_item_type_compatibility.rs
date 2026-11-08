//! UVM-SEQ-013 (SequenceItemTypeCompatibility): Statically verifies that
//! transaction item types between uvm_driver#(REQ) and uvm_sequencer#(REQ) are
//! type-compatible across connect_phase connections.

use sv_syntax::SyntaxKind::*;
use crate::support::slang_ast::kids;
use crate::expr_types::expr_types;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::config_db_calls::{strip_parens};

/// The first type argument in a printed specialization (`a#(B,C)` -> `B`).
fn first_type_arg(t: &str) -> Option<&str> {
    let open = t[t.find('#')?..].find('(')? + t.find('#')?;
    let rest = &t[open + 1..];
    let close = rest.find(')')?;
    let end = rest.find(',').filter(|&c| c < close).unwrap_or(close);
    let p = rest[..end].trim();
    (!p.is_empty()).then_some(p)
}

pub(crate) fn item_type_compatibility(c: &Cst, out: &mut Sink) {
    let types = expr_types(c);
    let ix = c.index();
    for f in c.nodes(FunctionDecl) {
        if crate::support::slang_ast::last_name(c, f) != "connect_phase" {
            continue;
        }
        // A class's method (in the class, or defined out of it).
        let in_class = c.parent(f).is_some_and(|p| c.kind(p) == ClassDecl) || c.toks(f).any(|t| c.text_of(t) == "::");
        if !in_class || crate::support::slang_ast::is_library(c, f) {
            continue;
        }
        // The UVM model's classes: not generic ones (their specializations
        // are not walked here).
        let owner = crate::support::slang_ast::ancestors(c, f).find(|&a| c.kind(a) == ClassDecl);
        if owner.is_some_and(|k| c.tree.child(k, Header).and_then(|h| c.tree.child(h, ParamList)).is_some()) {
            continue;
        }
        let mut todo = vec![f];
        while let Some(n) = todo.pop() {
            todo.extend(kids(c, n));
            if c.kind(n) != CallExpr {
                continue;
            }
            let Some((_, name, Some(recv))) = crate::support::slang_ast::call_parts(c, n) else { continue };
            if name != "connect" {
                continue;
            }
            let Some(&arg) = crate::expr_types::call_args(c, n).first() else { continue };
            let arg = strip_parens(c, arg);
            let (Some(rt), Some(at)) = (types.get(&(ix, recv)), types.get(&(ix, arg))) else { continue };
            let (Some(r1), Some(r2)) = (first_type_arg(&rt.text), first_type_arg(&at.text)) else { continue };
            if r1 == r2 {
                continue;
            }
            let Some(t) = c.first_tok(n) else { continue };
            out.add(
                "UVM-SEQ-013",
                "SequenceItemTypeCompatibility",
                Severity::Warning,
                format!(
                    "Transaction item type mismatch in connect(): driver port type '{}' (REQ={r1}) is connected to sequencer export type '{}' (REQ={r2}). Mismatched item parameters will cause runtime slicing or dynamic cast failures.",
                    rt.text, at.text
                ),
                c.loc(t),
            );
        }
    }
}
