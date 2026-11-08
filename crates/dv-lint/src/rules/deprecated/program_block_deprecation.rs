//! SV-DEP-001 (ProgramBlockDeprecation): Disallow the obsolete SystemVerilog
//! 'program' block.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn program_block(c: &Cst, s: &mut Sink) {
    for n in c.nodes(ProgramDecl) {
        let Some(kw) = c.tok(n, "program") else { continue };
        // A program with no name is a syntax error that uvmcheck reports
        // nothing else on.
        let name = c.tree.child(n, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        s.add(
            "SV-DEP-001",
            "ProgramBlockDeprecation",
            Severity::Error,
            format!(
                "The 'program' block '{name}' is obsolete and deprecated in modern SystemVerilog/UVM (IEEE 1800 Annex C / IEEE 1800-2017 § 24). \
                 Program blocks cause premature simulation termination ($finish when initial blocks end) and reactive scheduling hazards. \
                 Replace with 'module' and clocking blocks or standard packages."
            ),
            c.loc(kw),
        )
        .fixes
        .extend(
            [
                crate::fix::with_safety(
                    c.token_fix(kw, kw, "module", "Replace 'program' with 'module'"),
                    crate::fix::FixSafety::Unsafe,
                ),
                c.tok(n, "endprogram").and_then(|e| {
                    crate::fix::with_safety(
                        c.token_fix(e, e, "endmodule", "Replace 'endprogram' with 'endmodule'"),
                        crate::fix::FixSafety::Unsafe,
                    )
                }),
            ]
            .into_iter()
            .flatten(),
        );
    }
}
