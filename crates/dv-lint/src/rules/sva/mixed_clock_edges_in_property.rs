//! SV-SVA-003 (MixedClockEdgesInProperty): Flags SVA properties or assertions
//! that mix both 'posedge' and 'negedge' clocking events.

use sv_syntax::NodeId;
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;
use crate::support::sva_cov::{assert_kw};

fn edges(c: &Cst, n: NodeId) -> (bool, bool, Option<u32>) {
    let (mut pos, mut neg, mut first) = (false, false, None);
    for e in subtree(c, n) {
        if c.kind(e) != EventExpr {
            continue;
        }
        let Some(t) = c.keyword(e) else { continue };
        match c.text_of(t) {
            "posedge" => pos = true,
            "negedge" => neg = true,
            _ => continue,
        }
        first.get_or_insert(t);
    }
    (pos, neg, first)
}

pub(crate) fn mixed_edges(c: &Cst, s: &mut Sink) {
    let report = |s: &mut Sink, what: String, at: Option<u32>, fallback: Option<u32>| {
        let Some(t) = at.or(fallback) else { return };
        s.add(
            "SV-SVA-003",
            "MixedClockEdgesInProperty",
            Severity::Warning,
            format!(
                "{what} contains mixed clock edge triggers (both 'posedge' and 'negedge'). \
                 In SystemVerilog assertion verification (IEEE 1800 § 16.13), mixing positive and negative clock \
                 edges within the same assertion introduces half-cycle race conditions and causes severe state space \
                 explosion in formal verification (FPV) engines."
            ),
            c.loc(t),
        );
    };
    for p in c.nodes(PropertyDecl) {
        let (pos, neg, first) = edges(c, p);
        if pos && neg {
            let nm = c.tree.child(p, Name).and_then(|x| c.first_tok(x));
            report(s, format!("Property '{}'", decl_name(c, p)), first, nm);
        }
    }
    for a in c.nodes(ConcurrentAssert) {
        let (pos, neg, first) = edges(c, a);
        if pos && neg {
            let kw = assert_kw(c, a);
            report(s, format!("Assertion '{kw} property'"), first, c.tok(a, kw));
        }
    }
}
