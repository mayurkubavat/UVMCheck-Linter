//! SV-SVA-005 (MultiClockConcurrentAssertionDomain): Disallows concurrent
//! assertions mixing multiple asynchronous clock domains without formal CDC
//! qualification.

use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::BTreeSet;
use svlang::syntax::SyntaxKind::*;

pub(crate) fn multi_clock(c: &Cst, s: &mut Sink) {
    for p in c.nodes(PropertyDecl) {
        let mut clocks = BTreeSet::new();
        for e in subtree(c, p) {
            if c.kind(e) != EventExpr {
                continue;
            }
            let Some(&x) = kids(c, e).first() else { continue };
            let t = slang_trimmed(c, x);
            if !t.is_empty() {
                clocks.insert(t);
            }
        }
        if clocks.len() <= 1 {
            continue;
        }
        let list = clocks.iter().map(|x| format!("'{x}'")).collect::<Vec<_>>().join(", ");
        let Some(t) = c.tree.child(p, Name).and_then(|x| c.first_tok(x)) else { continue };
        s.add(
            "SV-SVA-005",
            "MultiClockConcurrentAssertionDomain",
            Severity::Error,
            format!(
                "Property '{}' mixes multiple distinct clock domains ({list}) without explicit clock domain crossing (CDC) synchronization. Under IEEE 1800 § 16.13 and \
                 Under IEEE 1800 § 16.13 assertion rules, multi-clocked assertions without formal CDC qualification introduce simulation \
                 race conditions, non-deterministic sampling, and exponential formal verification state explosion.",
                decl_name(c, p)
            ),
            c.loc(t),
        );
    }
}
