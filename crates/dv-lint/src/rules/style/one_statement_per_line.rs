//! SV-STY-011 (OneStatementPerLine): Prohibit multiple procedural statements on
//! the same line separated by semicolons.

use crate::support::source_layout::Buffers;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

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
            )
            .fixes
            .extend(split_fix(c, cur, next));
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

/// Moves the next statement onto its own line at the same indent. Only
/// plain spaces may sit between the two statements; a comment there is left
/// for a person to place.
fn split_fix(c: &Cst, cur: u32, next: u32) -> Option<crate::fix::Fix> {
    let (a, b) = (&c.tree.tokens[cur as usize], &c.tree.tokens[next as usize]);
    if a.from_macro || b.from_macro || a.origin.file != b.origin.file {
        return None;
    }
    let text = c.tree.sources.get(a.origin.file as usize)?.as_bytes();
    let from = a.origin.offset as usize + a.text.len();
    let to = b.origin.offset as usize;
    if from > to || to > text.len() || text[from..to].iter().any(|&ch| ch != b' ' && ch != b'\t') {
        return None;
    }
    let mut line_start = from;
    while line_start > 0 && text[line_start - 1] != b'\n' {
        line_start -= 1;
    }
    let mut indent_end = line_start;
    while indent_end < text.len() && matches!(text[indent_end], b' ' | b'\t') {
        indent_end += 1;
    }
    let indent = std::str::from_utf8(&text[line_start..indent_end]).ok()?;
    Some(c.file_fix(a.origin.file, from, to, format!("\n{indent}"), "Put the next statement on its own line"))
}
