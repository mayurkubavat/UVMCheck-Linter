//! SV-COV-004 (IllegalBinOverlap): Detects overlapping values between regular
//! 'bins' and 'illegal_bins' or 'ignore_bins' within a coverpoint.

use crate::support::slang_ast::*;
use crate::support::procedural::start;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;
use crate::support::sva_cov::{bins_kw, bins_name, bins_values, label_of};

pub(crate) fn illegal_bin_overlap(c: &Cst, s: &mut Sink) {
    for cp in c.nodes(Coverpoint) {
        let cp_name = label_of(c, cp).unwrap_or_else(|| "unnamed_cp".into());
        let mut regular = Vec::new();
        let mut special = Vec::new();
        for b in kids(c, cp) {
            if c.kind(b) != Bins {
                continue;
            }
            let kw = bins_kw(c, b).to_string();
            let name = bins_name(c, b).map(|x| x.0).unwrap_or_else(|| "unnamed".into());
            let Some(loc) = start(c, b) else { continue };
            let vals: Vec<String> = bins_values(c, b).into_iter().map(|v| slang_trimmed(c, v)).filter(|v| !v.is_empty()).collect();
            let Some(vals) = std_unordered_order(&vals) else { continue };
            if kw == "bins" {
                regular.push((name, vals));
            } else if kw == "illegal_bins" || kw == "ignore_bins" {
                special.push((name, kw, vals, loc));
            }
        }
        for (sname, skw, svals, sloc) in &special {
            for (rname, rvals) in &regular {
                if let Some(v) = svals.iter().find(|v| rvals.contains(v)) {
                    s.add(
                        "SV-COV-004",
                        "IllegalBinOverlap",
                        Severity::Error,
                        format!(
                            "Coverpoint '{cp_name}': overlapping value '{v}' detected between regular bins '{rname}' and {skw} '{sname}'. In SystemVerilog (IEEE 1800 § 19.5), regular bins must not overlap with illegal_bins or ignore_bins, creating non-deterministic bin sampling and simulator errors."
                        ),
                        *sloc,
                    );
                }
            }
        }
    }
}
