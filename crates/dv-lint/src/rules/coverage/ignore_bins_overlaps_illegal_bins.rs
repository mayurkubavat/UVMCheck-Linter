//! SV-COV-013 (IgnoreBinsOverlapsIllegalBins): Flags coverage models where the
//! same value or range is specified in both 'ignore_bins' and 'illegal_bins'
//! within a coverpoint.

use std::collections::HashMap;
use dv_model::Loc;
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;
use crate::support::sva_cov::{bins_kw, bins_name, bins_values, label_of};

pub(crate) fn ignore_illegal_overlap(c: &Cst, s: &mut Sink) {
    for cp in c.nodes(Coverpoint) {
        let cp_name = label_of(c, cp).unwrap_or_else(|| "<unnamed_coverpoint>".into());
        let mut ignore: HashMap<String, (String, Loc)> = HashMap::new();
        let mut illegal: Vec<(String, (String, Loc))> = Vec::new();
        for b in kids(c, cp) {
            if c.kind(b) != Bins {
                continue;
            }
            let kw = bins_kw(c, b);
            let name = bins_name(c, b).map(|x| x.0).unwrap_or_default();
            for v in bins_values(c, b) {
                let Some(t) = c.first_tok(v) else { continue };
                let mut val = c.text_of(t).trim_start_matches('\\').to_string();
                if val.is_empty() {
                    val = slang_text(c, v);
                }
                let info = (name.clone(), c.loc(t));
                if kw == "ignore_bins" {
                    ignore.insert(val, info);
                } else if kw == "illegal_bins" {
                    match illegal.iter_mut().find(|x| x.0 == val) {
                        Some(x) => x.1 = info,
                        None => illegal.push((val, info)),
                    }
                }
            }
        }
        for (val, (ill, loc)) in illegal {
            if let Some((ign, _)) = ignore.get(&val) {
                s.add(
                    "SV-COV-013",
                    "IgnoreBinsOverlapsIllegalBins",
                    Severity::Error,
                    format!(
                        "Coverage value '{val}' is specified in both 'ignore_bins' ({ign}) and 'illegal_bins' ({ill}) within coverpoint '{cp_name}'. \
                         In IEEE 1800 § 19.5 coverage methodology, overlapping \
                         ignore and illegal bins creates contradictory coverage semantics and causes simulator runtime errors."
                    ),
                    loc,
                );
            }
        }
    }
}
