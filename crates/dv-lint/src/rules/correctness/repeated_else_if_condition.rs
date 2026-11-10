//! SV-BUG-002 (RepeatedElseIfCondition): Flags a condition that repeats an
//! earlier condition of the same if/else-if chain, which makes the later branch
//! dead (Sonar S1862).

use crate::support::class_items::user_code;
use crate::support::data_items::unparen;
use crate::support::sema_facts::ast_ok;
use crate::support::type_text::expr_text;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

/// The condition of an if statement when it is one plain expression.
fn if_condition(c: &Cst, n: NodeId) -> Option<NodeId> {
    let e = c.tree.children(n).find(|&k| crate::support::sema_facts::is_expr(c.kind(k)))?;
    if c.kind(unparen(c, e)) == MatchExpr || c.toks(e).any(|t| c.text_of(t) == "&&&") {
        return None;
    }
    Some(e)
}

fn else_if(c: &Cst, n: NodeId) -> Option<NodeId> {
    let e = c.tree.child(n, ElseClause)?;
    c.tree.children(e).next().filter(|&k| c.kind(k) == IfStmt)
}

pub(crate) fn repeated_else_if(c: &Cst, s: &mut Sink) {
    for head in c.nodes(IfStmt) {
        if c.parent(head).is_some_and(|p| c.kind(p) == ElseClause) || !user_code(c, head) {
            continue;
        }
        let mut conds = Vec::new();
        let mut cur = Some(head);
        while let Some(n) = cur {
            if let Some(e) = if_condition(c, n) {
                conds.push(e);
            }
            cur = else_if(c, n);
        }
        let keys: Vec<String> = conds.iter().map(|&e| c.tree.bare_text(unparen(c, e))).collect();
        for i in 1..conds.len() {
            let text = expr_text(c, conds[i]);
            if ["$urandom", "$random", "randomize", "try_", "++", "--"].iter().any(|v| text.contains(v)) {
                continue;
            }
            let Some(j) = (0..i).find(|&j| keys[j] == keys[i]) else { continue };
            if !ast_ok(c, conds[i]) {
                continue;
            }
            let (Some(at), Some(first)) = (c.first_tok(unparen(c, conds[i])), c.first_tok(unparen(c, conds[j]))) else {
                continue;
            };
            s.add(
                "SV-BUG-002",
                "RepeatedElseIfCondition",
                Severity::Warning,
                format!(
                    "Condition '{text}' already appears at line {} in the same if/else-if chain, so this branch can \
                     never run. It usually hides a case that was meant to be handled here.",
                    c.loc(first).line
                ),
                c.loc(at),
            );
        }
    }
}
