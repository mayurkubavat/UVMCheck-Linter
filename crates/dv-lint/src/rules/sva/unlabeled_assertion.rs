//! SVA-LBL-001 (UnlabeledAssertion): Immediate and concurrent assertions must
//! declare an explicit label prefix (e.g. 'check_lbl: assert ...').

use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

pub(crate) fn unlabeled_assertion(c: &Cst, s: &mut Sink) {
    let mut found: Vec<(NodeId, bool)> = c.nodes(AssertStmt).into_iter().map(|n| (n, false)).collect();
    found.extend(c.nodes(ConcurrentAssert).into_iter().map(|n| (n, true)));
    for (n, concurrent) in found {
        if c.tree.child(n, StmtLabel).is_some() {
            continue;
        }
        let Some(kw) = c.keyword(n) else { continue };
        let kw_text = c.text_of(kw);
        let msg = if concurrent {
            let full = match c.toks(n).nth(1).map(|t| c.text_of(t)) {
                Some(p @ ("property" | "sequence")) => format!("{kw_text} {p}"),
                _ => kw_text.to_string(),
            };
            format!(
                "Concurrent assertion '{full}' lacks an explicit label prefix (e.g. 'check_handshake: {full} (...)'). \
                 Assertions without labels cannot be targeted by simulator severity/assertion control commands (+assert_filter, $assertoff)."
            )
        } else {
            format!(
                "Immediate assertion '{kw_text}' lacks an explicit label prefix (e.g. 'check_val: {kw_text} (...)'). \
                 Unlabeled assertions impede automated regression failure triage, ticket assignment, and coverage correlation."
            )
        };
        s.add("SVA-LBL-001", "UnlabeledAssertion", Severity::Warning, msg, c.loc(kw));
    }
}
