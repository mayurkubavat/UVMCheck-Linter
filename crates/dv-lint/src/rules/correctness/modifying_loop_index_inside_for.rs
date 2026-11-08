//! SV-COR-011 (ModifyingLoopIndexInsideFor): Flags modifications to the loop
//! control variable inside a 'for' loop body.

use std::collections::HashSet;
use sv_syntax::NodeId;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;
use crate::support::for_loops::{for_parts, for_vars, is_ident_name, keyword_is, kids, op_text};

pub(crate) fn loop_index_modified(c: &Cst, s: &mut Sink) {
    fn walk(c: &Cst, n: NodeId, vars: &HashSet<String>, s: &mut Sink) {
        let target = match c.kind(n) {
            PostfixExpr | PrefixExpr if matches!(op_text(c, n), "++" | "--") => {
                kids(c, n).first().copied().zip(c.first_tok(n))
            }
            BinaryExpr if matches!(op_text(c, n), "=" | "+=" | "-=") => {
                kids(c, n).first().copied().zip(c.toks(n).next())
            }
            _ => None,
        };
        if let Some((e, at)) = target {
            if is_ident_name(c, e) {
                let name = c.tree.bare_text(e);
                if vars.contains(&name) {
                    s.add(
                        "SV-COR-011",
                        "ModifyingLoopIndexInsideFor",
                        Severity::Warning,
                        format!(
                            "Loop control variable '{name}' is procedurally modified inside the body of the 'for' loop. \
                             Altering the loop index within the loop body conflicts with the loop iteration step logic, \
                             easily causing infinite loops, skipped elements, or subtle indexing off-by-one errors. \
                             Move index adjustments into the loop update clause or use a 'while' loop."
                        ),
                        c.loc(at),
                    );
                }
            }
        }
        if c.kind(n) == LoopStmt && keyword_is(c, n, "for") {
            return;
        }
        for k in c.tree.children(n) {
            walk(c, k, vars, s);
        }
    }
    for l in c.nodes(LoopStmt).into_iter().filter(|&l| keyword_is(c, l, "for")) {
        let (init, _, body) = for_parts(c, l);
        let vars: HashSet<String> = for_vars(c, &init, false).into_iter().collect();
        if let (false, Some(body)) = (vars.is_empty(), body) {
            walk(c, body, &vars, s);
        }
    }
}
