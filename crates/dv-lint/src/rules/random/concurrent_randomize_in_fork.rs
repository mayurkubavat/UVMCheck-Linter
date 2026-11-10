//! SV-RND-016 (ConcurrentRandomizeInFork): Disallows concurrent calls to
//! .randomize() on the same object handle across parallel fork threads without
//! synchronization.

use crate::support::class_items::subtree;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::HashSet;
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

fn randomize_target(c: &Cst, e: NodeId, out: &mut Vec<(String, u32)>) {
    match c.kind(e) {
        MemberExpr | ScopedExpr => {
            let Some(name) = c.toks(e).last() else { return };
            if c.text_of(name) != "randomize" {
                return;
            }
            let Some(left) = c.tree.children(e).next() else { return };
            let target = c.last_text(left);
            if !target.is_empty() {
                out.push((target.to_string(), name));
            }
        }
        CallExpr => {
            if let Some(callee) = c.tree.children(e).next() {
                randomize_target(c, callee, out);
            }
        }
        _ => {}
    }
}

pub(crate) fn concurrent_randomize(c: &Cst, s: &mut Sink) {
    for fork in c.nodes(ParBlock) {
        let mut seen: HashSet<String> = HashSet::new();
        let mut reported: HashSet<String> = HashSet::new();
        for item in c.items(fork) {
            let mut targets = Vec::new();
            for n in subtree(c, item) {
                if c.kind(n) != CallExpr {
                    continue;
                }
                // `x.randomize() with {...}` is slang's
                // ArrayOrRandomizeMethodExpression around the call.
                if let Some(callee) = c.tree.children(n).next() {
                    randomize_target(c, callee, &mut targets);
                }
            }
            for (t, at) in targets {
                if !seen.insert(t.clone()) && reported.insert(t.clone()) {
                    s.add(
                        "SV-RND-016",
                        "ConcurrentRandomizeInFork",
                        Severity::Warning,
                        format!(
                            "Concurrent call to '.randomize()' on the same object handle '{t}' across parallel fork...join threads without synchronization. Randomizing a shared object concurrently causes solver race conditions, state corruption, and non-deterministic constraint failures."
                        ),
                        c.loc(at),
                    );
                }
            }
        }
    }
}
