//! SV-CMP-001 (CompilerError): Reports the SystemVerilog compiler's errors
//! (syntax, elaboration and type errors) as findings, so they appear in every
//! report and in the editor next to the lint findings.

use crate::syntax::Cst;
use crate::{Severity, Sink};

type SemaDiags = std::sync::Arc<Vec<sv_sema::names::Diag>>;

/// The native front end's semantic errors for the compile, computed once.
fn semantic_diags(c: &Cst) -> SemaDiags {
    crate::shared::get("semantic_diags", c.compile_key(), || {
        crate::shared::sema_checks(c).diags(c.all, &crate::shared::elaboration(c))
    })
}

/// The compiler's errors as findings: preprocessor, syntax and semantic
/// errors of the native front end (its messages, not slang's).
pub(crate) fn compiler_errors(c: &Cst, out: &mut Sink) {
    let mut add = |msg: &str, loc: dv_model::Loc| {
        out.add("SV-CMP-001", "CompilerError", Severity::Error, msg.to_string(), loc);
    };
    for d in &c.tree.pp_diagnostics {
        add(&d.message, c.origin_loc(d.origin));
    }
    let n = c.tree.tokens.len();
    for e in &c.tree.errors {
        if n == 0 {
            continue;
        }
        let tok = (e.token as usize).min(n - 1) as u32;
        add(&e.message, c.loc(tok));
    }
    let ix = c.index();
    for d in semantic_diags(c).iter().filter(|d| d.tree == ix) {
        let Some((tok, _)) = c.tree.token_bounds(d.node) else { continue };
        add(&d.message, c.loc(tok));
    }
}
