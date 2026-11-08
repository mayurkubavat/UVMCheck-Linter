//! SV-RND-013 (UnrestoredRandMode): Flags '.rand_mode(0)' or
//! '.constraint_mode(0)' calls that are never restored with '1' in the routine.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn unrestored_rand_mode(c: &Cst, s: &mut Sink) {
    for f in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)) {
        let Some(body) = c.tree.child(f, Body) else { continue };
        let routine = c.tree.child(f, Name).map(|n| c.last_text(n).to_string()).unwrap_or_default();
        let mut disabled: Option<u32> = None;
        let mut restored = false;
        let mut method = "";
        for item in c.items(body) {
            let text = c.text(item);
            let first = c.first_tok(item);
            if text.contains("rand_mode(0)") || text.contains("rand_mode(1'b0)") {
                disabled = first.or(disabled);
                method = "rand_mode";
            } else if text.contains("rand_mode(1)") || text.contains("rand_mode(1'b1)") {
                restored = true;
            }
            if text.contains("constraint_mode(0)") || text.contains("constraint_mode(1'b0)") {
                disabled = first.or(disabled);
                method = "constraint_mode";
            } else if text.contains("constraint_mode(1)") || text.contains("constraint_mode(1'b1)") {
                restored = true;
            }
        }
        let Some(at) = disabled.filter(|_| !restored) else { continue };
        s.add(
            "SV-RND-013",
            "UnrestoredRandMode",
            Severity::Warning,
            format!(
                "Random mode or constraint was disabled via '.{method}(0)' in '{routine}()' but was never restored with '.{method}(1)'. Permanently leaving random fields or constraints disabled causes silent stimulus degradation across subsequent transactions."
            ),
            c.loc(at),
        );
    }
}
