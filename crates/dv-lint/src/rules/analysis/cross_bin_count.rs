//! SV-COV-018 (CrossBinCountTooLarge): Counts the bins of each covergroup
//! cross, as a simulator would build them (explicit bins, `bins x[N]`,
//! `bins x[] = {...}` one per value, or automatic bins: 2^width up to
//! option.auto_bin_max, 64 by default), and flags crosses with more than
//! 10,000: that many bins are rarely all hit, so the coverage goal is
//! unreachable in practice. Crosses with their own bins or ignore_bins, and
//! coverpoints whose count is not a constant, are left out.

use std::collections::HashMap;

use super::values::{const_int, type_range, Set};
use crate::support::ast::resolve;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

const LIMIT: u128 = 10_000;

/// `option.auto_bin_max = N;` directly under `n`.
fn auto_bin_max(c: &Cst, n: NodeId) -> Option<u128> {
    for o in c.tree.children(n).filter(|&k| c.kind(k) == CoverageOption) {
        let Some(a) = c.tree.children(o).next() else { continue };
        let k: Vec<NodeId> = c.tree.children(a).collect();
        let [lhs, rhs] = k[..] else { continue };
        if c.trimmed(lhs).replace(' ', "") == "option.auto_bin_max" {
            return const_int(c, rhs).and_then(|v| u128::try_from(v).ok());
        }
    }
    None
}

/// The values of a bins' range list, when every element is a constant.
fn range_values(c: &Cst, list: NodeId) -> Option<Set> {
    let mut s = Set::empty();
    for e in c.tree.children(list) {
        let v = if c.kind(e) == OpenRange {
            let k: Vec<NodeId> = c.tree.children(e).collect();
            let [lo, hi] = k[..] else { return None };
            Set::range(const_int(c, lo)?, const_int(c, hi)?)
        } else {
            Set::point(const_int(c, e)?)
        };
        s = s.union(&v);
    }
    Some(s)
}

/// Bits of the value `expr` names, from its declaration.
fn width_of(c: &Cst, expr: NodeId) -> Option<u32> {
    if c.kind(expr) != NameExpr {
        return None;
    }
    let d = resolve(c, expr, &c.tree.bare_text(expr))?;
    type_range(c, d.dtype?).map(|(_, w)| w)
}

/// How many bins a coverpoint has, or None when that is not a constant.
fn coverpoint_bins(c: &Cst, cp: NodeId, group_abm: u128) -> Option<u128> {
    let bins: Vec<NodeId> = c.tree.children(cp).filter(|&k| c.kind(k) == Bins).collect();
    let counted: Vec<NodeId> = bins
        .iter()
        .copied()
        .filter(|&b| {
            c.toks(b).next().map(|t| c.text_of(t)) == Some("bins") && !c.toks(b).any(|t| c.text_of(t) == "default")
        })
        .collect();
    if counted.is_empty() {
        let expr = c.tree.children(cp).find(|&k| c.kind(k) != StmtLabel)?;
        let w = width_of(c, expr)?;
        let abm = auto_bin_max(c, cp).unwrap_or(group_abm);
        return Some(if w >= 64 { abm } else { (1u128 << w).min(abm) });
    }
    let mut n = 0u128;
    for b in counted {
        let own: Vec<&str> = c.toks(b).map(|t| c.text_of(t)).collect();
        if own.contains(&"=>") || c.tree.children(b).any(|k| !matches!(c.kind(k), Name | Literal | RangeList)) {
            return None;
        }
        let array = own.contains(&"[");
        let size = c.tree.children(b).find(|&k| c.kind(k) == Literal);
        n = n.saturating_add(match (array, size) {
            (true, Some(s)) => u128::try_from(const_int(c, s)?).ok()?,
            (true, None) => range_values(c, c.tree.child(b, RangeList)?)?.count(),
            (false, _) => 1,
        });
    }
    Some(n)
}

pub(crate) fn cross_bin_count(c: &Cst, s: &mut Sink) {
    for x in c.nodes(Cross) {
        if c.tree.children(x).any(|k| c.kind(k) == Bins) {
            continue;
        }
        let Some(cg) = c.parent(x).filter(|&p| c.kind(p) == CovergroupDecl) else { continue };
        let group_abm = auto_bin_max(c, cg).unwrap_or(64);
        let mut labels: HashMap<String, NodeId> = HashMap::new();
        for cp in c.tree.children(cg).filter(|&k| c.kind(k) == Coverpoint) {
            if let Some(l) = c.tree.child(cp, StmtLabel).and_then(|l| c.first_tok(l)) {
                labels.insert(c.text_of(l).to_string(), cp);
            }
        }
        let mut parts = Vec::new();
        let mut total = 1u128;
        let mut ok = true;
        for item in c.tree.children(x).filter(|&k| c.kind(k) == NameExpr) {
            let name = c.tree.bare_text(item);
            let n = match labels.get(&name) {
                Some(&cp) => coverpoint_bins(c, cp, group_abm),
                // A variable crossed directly: its automatic bins.
                None => width_of(c, item).map(|w| if w >= 64 { group_abm } else { (1u128 << w).min(group_abm) }),
            };
            let Some(n) = n else {
                ok = false;
                break;
            };
            total = total.saturating_mul(n);
            parts.push(format!("'{name}' ({n})"));
        }
        if !ok || parts.len() < 2 || total <= LIMIT {
            continue;
        }
        let at =
            c.tree.child(x, StmtLabel).and_then(|l| c.first_tok(l)).or_else(|| c.keyword(x)).or_else(|| c.first_tok(x));
        let Some(at) = at else { continue };
        let label = c
            .tree
            .child(x, StmtLabel)
            .and_then(|l| c.first_tok(l))
            .map(|t| c.text_of(t).to_string())
            .unwrap_or_else(|| "cross".into());
        s.add(
            "SV-COV-018",
            "CrossBinCountTooLarge",
            Severity::Warning,
            format!(
                "Cross '{label}' has {total} bins ({}): more than {LIMIT}, so full cross coverage is out of reach in practice. \
Cross fewer or coarser bins (group values with explicit bins), or restrict the cross with binsof ... intersect.",
                parts.join(" x ")
            ),
            c.loc(at),
        );
    }
}
