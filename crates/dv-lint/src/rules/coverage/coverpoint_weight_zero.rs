//! SV-COV-014 (CoverpointWeightZero): Flags covergroups or coverpoints where
//! 'option.weight' or 'type_option.weight' is set to 0, silently excluding it
//! from coverage scores.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn weight_zero(c: &Cst, s: &mut Sink) {
    for n in c.nodes(CoverageOption) {
        let Some(e) = c.items(n).into_iter().next() else { continue };
        let text: String = c.text(e).chars().filter(|&ch| ch != '\n' && ch != '\r').collect();
        if !text.contains("weight") || c.kind(e) != BinaryExpr {
            continue;
        }
        if !c.toks(e).next().is_some_and(|t| c.text_of(t) == "=") {
            continue;
        }
        let Some(right) = c.tree.children(e).nth(1) else { continue };
        let r: String = c.text(right).chars().filter(|ch| !ch.is_ascii_whitespace()).collect();
        if !matches!(r.as_str(), "0" | "1'b0" | "'0" | "32'd0") {
            continue;
        }
        let target = if text.contains("type_option") { "type_option.weight" } else { "option.weight" };
        let Some(first) = c.first_tok(e) else { continue };
        s.add(
            "SV-COV-014",
            "CoverpointWeightZero",
            Severity::Warning,
            format!(
                "Coverage option 'weight' is explicitly configured with value 0 ('{target} = {r}'). \
                 In IEEE 1800 § 19.5 coverage methodology, weight = 0 silently \
                 excludes this covergroup or coverpoint from cumulative functional coverage metric computations, \
                 creating a misleading impression of active coverage tracking without contributing to tape-out closure."
            ),
            c.loc(first),
        );
    }
}
