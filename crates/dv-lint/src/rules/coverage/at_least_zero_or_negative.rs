//! SV-COV-011 (AtLeastZeroOrNegative): Flags option.at_least or
//! type_option.at_least set to 0 or negative values in covergroups/coverpoints.

use crate::support::procedural::start;
use crate::support::slang_ast::*;
use crate::support::sva_cov::{option_parts, option_target};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn at_least_zero(c: &Cst, s: &mut Sink) {
    for o in c.nodes(CoverageOption) {
        let Some((e, rhs)) = option_parts(c, o) else { continue };
        if !slang_text(c, e).contains("at_least") {
            continue;
        }
        let r = slang_trimmed(c, rhs);
        if !(matches!(r.as_str(), "0" | "'0" | "32'd0" | "32'b0") || r.starts_with('-')) {
            continue;
        }
        let target = option_target(c, o);
        let Some(loc) = start(c, o) else { continue };
        s.add(
            "SV-COV-011",
            "AtLeastZeroOrNegative",
            Severity::Warning,
            format!(
                "Coverage construct '{target}' specifies non-positive 'at_least' threshold ('{r}'). In IEEE 1800 § 19.5, 'option.at_least' and 'type_option.at_least' define \
                 the minimum number of hit occurrences required for a bin to be considered covered and must be an integer >= 1. \
                 Setting at_least <= 0 causes bins to be trivially covered without verification or invalidates coverage metrics."
            ),
            loc,
        );
    }
}
