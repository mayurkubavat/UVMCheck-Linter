//! SV-COV-006 (CovergroupCrossBinExplosion): Disallows crossing 3 or more
//! coverpoints without 'ignore_bins' or 'binsof' exclusion filters.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;
use crate::support::cross_labels::{cross_parts, label};

pub(crate) fn cross_explosion(c: &Cst, s: &mut Sink) {
    for n in c.nodes(Cross) {
        let (items, members, kw) = cross_parts(c, n);
        if items < 3 {
            continue;
        }
        let filtered = members.iter().any(|&m| {
            let t = c.text(m);
            t.contains("ignore_bins") || t.contains("binsof") || t.contains("illegal_bins")
        });
        let Some(kw) = kw.filter(|_| !filtered) else { continue };
        let name = label(c, n).unwrap_or_else(|| "unnamed_cross".into());
        s.add(
            "SV-COV-006",
            "CovergroupCrossBinExplosion",
            Severity::Error,
            format!(
                "Covergroup cross '{name}' crosses {items} coverpoints without 'ignore_bins' or 'binsof' filters. \
                 Unfiltered 3-way or higher cross coverage creates a combinatorial explosion of auto-bins, \
                 exhausting simulator memory and bloating coverage databases. \
                 Apply 'ignore_bins' or 'binsof' expressions to exclude unreachable or irrelevant cross combinations."
            ),
            c.loc(kw),
        );
    }
}
