//! SV-STY-011 (OneStatementPerLine): Prohibit multiple procedural statements on
//! the same line separated by semicolons.

use sv_syntax::NodeId;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;
use crate::support::source_layout::{Buffers};

/// The first token slang gives a statement. A statement the parser could
/// not read (tokens only, no expression) is, after a stray `.`, slang's
/// empty statement at its `;`; other such statements are left out.
fn recovered_first(c: &Cst, n: NodeId) -> Option<u32> {
    let (a, e) = c.tree.token_bounds(n)?;
    if c.kind(n) == ExprStmt && c.tree.children(n).next().is_none() {
        return (c.text_of(a) == "." && c.text_of(e) == ";").then_some(e);
    }
    Some(a)
}

fn check_items(c: &Cst, s: &mut Sink, b: &Buffers, items: &[NodeId]) {
    let mut last_reported = 0;
    for w in items.windows(2) {
        let (Some(cur), Some(next)) = (c.tree.token_bounds(w[0]).map(|x| x.1), recovered_first(c, w[1])) else {
            continue;
        };
        if b.of(cur) != b.of(next) {
            continue;
        }
        let l = b.line(cur);
        if l == b.line(next) && l != last_reported {
            last_reported = l;
            s.add(
                "SV-STY-011",
                "OneStatementPerLine",
                Severity::Warning,
                format!(
                    "Multiple procedural statements found on line {l}. By convention, each statement must reside on its own line to ensure readable diffs and deterministic interactive debugging."
                ),
                c.loc(next),
            );
        }
    }
}

pub(crate) fn one_statement_per_line(c: &Cst, s: &mut Sink) {
    let b = Buffers::new(c);
    for n in c.nodes(SeqBlock).into_iter().chain(c.nodes(ParBlock)).chain(c.nodes(Body)) {
        let items = c.items(n);
        if items.len() >= 2 {
            check_items(c, s, &b, &items);
        }
    }
}
