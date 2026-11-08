//! SV-STY-009 (ExternMethodThreshold): Enforces 'extern' declaration for class
//! methods exceeding N lines of code (default: 5 lines).

use crate::support::source_layout::{routine_name, routines, Buffers};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn extern_method_threshold(c: &Cst, s: &mut Sink) {
    const THRESHOLD: u32 = 5;
    for (n, kw) in routines(c) {
        if !c.parent(n).is_some_and(|p| c.kind(p) == ClassDecl) {
            continue;
        }
        if c.toks(n).any(|t| c.text_of(t) == "extern") {
            continue;
        }
        let Some((_, last)) = c.tree.token_bounds(n) else { continue };
        let lines = Buffers::new(c);
        let (a, b) = (lines.line(kw), lines.line(last));
        let count = if b >= a { b - a + 1 } else { 1 };
        if count > THRESHOLD {
            s.add(
                "SV-STY-009",
                "ExternMethodThreshold",
                Severity::Warning,
                format!(
                    "Method '{}' implemented inline inside class spans {count} lines (threshold: {THRESHOLD} lines). In common SystemVerilog style guidelines, large method implementations should be moved outside the class declaration using 'extern' to maintain a clean class API.",
                    routine_name(c, n)
                ),
                c.loc(kw),
            );
        }
    }
}
