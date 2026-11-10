//! SV-COV-002 (ZeroWeightCoverpoint): Flags covergroups, coverpoints, or
//! crosses configured with option.weight or type_option.weight set to 0.

use crate::support::procedural::start;
use crate::support::slang_ast::*;
use crate::support::sva_cov::{option_parts, option_target};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind::*;

pub(crate) fn zero_weight(c: &Cst, s: &mut Sink) {
    for o in c.nodes(CoverageOption) {
        let Some((e, rhs)) = option_parts(c, o) else { continue };
        let text = slang_text(c, e);
        if !text.contains("weight") {
            continue;
        }
        let r = slang_trimmed(c, rhs);
        if !matches!(r.as_str(), "0" | "'0" | "32'd0" | "32'b0") {
            continue;
        }
        let target = option_target(c, o);
        let Some(loc) = start(c, o) else { continue };
        s.add(
            "SV-COV-002",
            "ZeroWeightCoverpoint",
            Severity::Warning,
            format!(
                "Coverage construct '{target}' specifies zero weight ('{text}'). \
                 Setting option.weight or type_option.weight to 0 causes this construct to contribute 0% to regression coverage calculation, masking untested logic."
            ),
            loc,
        );
    }
}
