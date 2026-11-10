//! SV-TIM-002 (MissingTimescaleInModule): Flags files and modules containing
//! procedural delays (#) that lack an explicit `timescale or timeunit
//! declaration.

use crate::support::class_items::{op, own};
use crate::support::data_items::main_files;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

fn has_timescale(src: &str) -> bool {
    for (word, len) in [("`timescale", 10), ("timeunit", 8)] {
        let mut pos = 0;
        while let Some(i) = src[pos..].find(word) {
            let at = pos + i;
            let start = src[..at].rfind('\n').map_or(0, |x| x + 1);
            let prefix = &src[start..at];
            if !prefix.contains("//") && !prefix.contains("/*") {
                return true;
            }
            pos = at + len;
        }
    }
    false
}

pub(crate) fn missing_timescale(c: &Cst, s: &mut Sink) {
    for f in main_files(c) {
        if has_timescale(&c.tree.sources[f as usize]) {
            continue;
        }
        // slang's DelaySyntax (`#d`, `##n`) and TimingControlStatementSyntax.
        // Sequence and property delays are not DelaySyntax.
        let mut first = None;
        let mut stack = vec![c.tree.root];
        while let Some(n) = stack.pop() {
            match c.kind(n) {
                SequenceDecl | PropertyDecl | SequenceExpr | PropertyExpr | ConcurrentAssert => continue,
                TimedStmt => {
                    first = Some(n);
                    break;
                }
                // `#1step` (a clocking skew) and a gate's `#(1, 2)` (a
                // parameter assignment to slang) are not delays.
                TimingControl
                    if matches!(op(c, n), "#" | "##")
                        && c.trimmed(n) != "#1step"
                        && !(c.parent(n).is_some_and(|p| matches!(c.kind(p), GateInst | Instantiation))
                            && own(c, n).get(1) == Some(&"(")) =>
                {
                    first = Some(n);
                    break;
                }
                _ => {}
            }
            let k: Vec<NodeId> = c.tree.children(n).collect();
            stack.extend(k.into_iter().rev());
        }
        let Some(at) = first.and_then(|n| c.first_tok(n)) else { continue };
        s.add(
            "SV-TIM-002",
            "MissingTimescaleInModule",
            Severity::Warning,
            "File contains procedural timing controls ('#delay') but omits a '`timescale' or 'timeunit' directive. Per IEEE 1800 § 3.14, omitting explicit timescale specifications causes simulators to adopt implementation-dependent default time units, introducing unpredictable timing skew and race conditions across VIP components."
                .into(),
            c.loc(at),
        );
    }
}
