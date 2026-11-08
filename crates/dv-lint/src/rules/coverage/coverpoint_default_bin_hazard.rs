//! SV-COV-009 (CoverpointDefaultBinHazard): Flags non-ignoring 'default' bins
//! in coverpoints that mask uncovered stimulus and give a false sense of
//! coverage closure.

use crate::support::names::name_tok;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn default_bin(c: &Cst, s: &mut Sink) {
    for n in c.nodes(Bins) {
        if c.tok(n, "bins").is_none() {
            continue;
        }
        let Some(eq) = c.tok(n, "=") else { continue };
        if !c.toks(n).skip_while(|&t| t != eq).nth(1).is_some_and(|t| c.text_of(t) == "default") {
            continue;
        }
        let Some(t) = name_tok(c, n) else { continue };
        s.add(
            "SV-COV-009",
            "CoverpointDefaultBinHazard",
            Severity::Warning,
            format!(
                "Coverpoint bin '{}' is defined with 'default' initializer. Catch-all 'default' bins group all uncharacterized stimulus values into a single bin, artificially inflating coverage closure metrics and masking unhandled corner cases. Use explicit bin ranges, 'ignore_bins', or 'illegal_bins' instead.",
                c.text_of(t)
            ),
            c.loc(t),
        );
    }
}
