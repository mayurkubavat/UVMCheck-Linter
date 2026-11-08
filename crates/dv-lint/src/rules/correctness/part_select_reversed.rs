//! SV-LANG-004 (PartSelectReversed): Detects reversed or inverted part-select
//! slicing on vectors (e.g. vec[0:7] on logic [7:0] vec).

use crate::support::literal_values::int_value;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

/// slang's RangeSelectReversed diagnostic (the select itself does not
/// bind), once per select, at its left bound.
pub(crate) fn part_select_reversed(c: &Cst, s: &mut Sink) {
    for sel in c.nodes(SelectExpr) {
        if !crate::support::sema_facts::reversed_select(c, sel) {
            continue;
        }
        let k: Vec<NodeId> = c.tree.children(sel).collect();
        let [_, l, r] = k[..] else { continue };
        let (Some(lv), Some(rv)) = (int_value(c, l), int_value(c, r)) else { continue };
        s.add(
            "SV-LANG-004",
            "PartSelectReversed",
            Severity::Error,
            format!(
                "Inverted part-select slicing: selection range [{lv}:{rv}] is reversed relative to declared vector endianness. In SystemVerilog (IEEE 1800 \u{a7} 7.4.3), part-select bounds must match the declared range endianness."
            ),
            c.loc_of(l),
        );
    }
}
