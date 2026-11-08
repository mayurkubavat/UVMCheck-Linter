//! SV-CON-014 (DisableNamedBlockInClass): Flags `disable <block>` and
//! `disable <task>` inside class methods.

use std::collections::HashSet;
use sv_syntax::{NodeId, SyntaxKind};
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::procedural::{own_methods, start};

fn named_blocks(c: &Cst, r: NodeId) -> HashSet<String> {
    let mut out = HashSet::new();
    for x in subtree(c, r) {
        if matches!(c.kind(x), SeqBlock | ParBlock) {
            for l in kids(c, x) {
                if matches!(c.kind(l), StmtLabel | EndLabel) {
                    if let Some(t) = c.toks(l).map(|t| c.text_of(t)).find(|t| *t != ":") {
                        out.insert(t.to_string());
                    }
                }
            }
        }
    }
    out
}

pub(crate) fn disable_in_class(c: &Cst, s: &mut Sink) {
    for cls in user_classes(c) {
        let cname = decl_name(c, cls);
        for m in own_methods(c, cls) {
            let blocks = named_blocks(c, m);
            for x in subtree(c, m) {
                if c.kind(x) != DisableStmt {
                    continue;
                }
                let toks = tok_texts(c, x);
                if toks.get(1) == Some(&"fork") {
                    continue;
                }
                let target = kids(c, x).first().map(|&n| c.tree.bare_text(n)).unwrap_or_else(|| {
                    toks.iter().skip(1).filter(|t| **t != ";").copied().collect::<String>()
                });
                let last = target.rsplit('.').next().unwrap_or("").to_string();
                let is_block = blocks.contains(&last);
                let is_task = !is_block && {
                    let chain = class_chain(c, cls);
                    chain.iter().any(|&k| {
                        kids(c, k).into_iter().any(|m2| {
                            (c.kind(m2) == TaskDecl || (c.kind(m2) == MethodPrototype && has_kw(c, m2, "task")))
                                && last_name(c, m2) == last
                        })
                    }) || c.nodes(TaskDecl).into_iter().any(|t| decl_name(c, t) == last && enclosing_class(c, t).is_none())
                };
                if !is_block && !is_task {
                    continue;
                }
                let Some(loc) = start(c, x) else { continue };
                s.add(
                    "SV-CON-014",
                    "DisableNamedBlockInClass",
                    Severity::Warning,
                    format!(
                        "'disable {last}' in class '{cname}' stops that {} in every object of the class, not just this one. \
                         Keep a process handle (process::self()) and call kill() on it, \
                         or use break/continue/return to leave a loop or task.",
                        if is_block { "block" } else { "task" }
                    ),
                    loc,
                );
            }
        }
    }
}
