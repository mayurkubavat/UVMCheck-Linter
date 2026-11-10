//! SV-COV-012 (WideCoverpointWithoutBins): Flags coverpoints that define no
//! explicit bins and omit option.auto_bin_max.

use crate::support::cross_labels::label;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::Child;
use svlang::syntax::SyntaxKind::*;

pub(crate) fn coverpoint_without_bins(c: &Cst, s: &mut Sink) {
    for n in c.nodes(Coverpoint) {
        let mut expr = None;
        let mut bins = 0;
        let mut auto_max = false;
        let mut in_body = false;
        for k in c.kids(n) {
            match *k {
                Child::Token(t) if c.text_of(t) == "{" => in_body = true,
                Child::Node(x) if in_body => match c.kind(x) {
                    Bins => bins += 1,
                    CoverageOption => auto_max |= c.text(x).contains("auto_bin_max"),
                    _ => {}
                },
                Child::Node(x) if expr.is_none() && !matches!(c.kind(x), StmtLabel | Attribute | DataType) => {
                    expr = Some(x)
                }
                _ => {}
            }
        }
        if bins > 0 || auto_max {
            continue;
        }
        let name = label(c, n).unwrap_or_else(|| "unnamed_cp".into());
        let e = expr.map(|x| c.trimmed(x)).unwrap_or_default();
        // slang anchors an implicit-typed coverpoint at its body `{` or closing `;`.
        let explicit = c.tree.child(n, DataType).is_some();
        let body = c.kids(n).iter().find_map(|k| match *k {
            Child::Token(t) if matches!(c.text_of(t), "{" | ";") => Some(t),
            _ => None,
        });
        let Some(t) = (if explicit { c.first_tok(n) } else { body.or_else(|| c.first_tok(n)) }) else { continue };
        s.add(
            "SV-COV-012",
            "WideCoverpointWithoutBins",
            Severity::Warning,
            format!(
                "Coverpoint '{name}' on expression '{e}' declares zero explicit bins and lacks 'option.auto_bin_max'. \
                 In SystemVerilog (IEEE 1800 § 19.5), coverpoints without explicit bins partition the state space into a default of 64 auto-bins. \
                 For wide vectors or integer types, this creates misleading coverage metrics where 64 arbitrary hits yield 100% closure across millions of untested values. \
                 Define explicit bins or configure 'option.auto_bin_max'."
            ),
            c.loc(t),
        );
    }
}
