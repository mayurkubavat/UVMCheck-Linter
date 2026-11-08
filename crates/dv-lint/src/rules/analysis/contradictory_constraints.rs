//! SV-RND-030 (ContradictoryConstraints): Flags rand variables whose hard
//! constraints leave no legal value, such as `len > 10` on a `bit [3:0]`, or
//! `x < 5` and `x inside {[8:9]}`: every randomize() of the class fails.
//! Bounds in one constraint block always count; bounds in different blocks
//! count together only when no user code calls constraint_mode(), since a
//! test may switch one of them off.

use std::collections::HashSet;

use super::constraint_sets::{classes, uses_constraint_mode, ClassConstraints};
use super::values::Set;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;

pub(crate) fn contradictory_constraints(_: &Design, csts: &[Cst], out: &mut Sink) {
    let mode = uses_constraint_mode(csts);
    for c in csts {
        for cc in classes(c) {
            let mut blocks: Vec<&str> = cc.atoms.iter().filter(|a| !a.soft).map(|a| a.block.as_str()).collect();
            blocks.dedup();
            // Each block on its own, then (without constraint_mode) all of them.
            let scopes: Vec<Option<&str>> = blocks.iter().map(|b| Some(*b)).chain((!mode).then_some(None)).collect();
            let mut vars: Vec<&String> = cc.atoms.iter().filter(|a| !a.soft).map(|a| &a.var).collect();
            vars.sort();
            vars.dedup();
            let mut done = HashSet::new();
            for scope in scopes {
                for v in &vars {
                    if !done.contains(*v) && check(c, &cc, v, scope, out) {
                        done.insert((*v).clone());
                    }
                }
            }
        }
    }
}

/// Reports `var` when its bounds (in `scope`) leave nothing; true when it did.
fn check(c: &Cst, cc: &ClassConstraints, var: &str, scope: Option<&str>, out: &mut Sink) -> bool {
    let (range, ty) = &cc.vars[var];
    let mut s: Set = range.clone();
    let mut used = Vec::new();
    for a in cc.atoms.iter().filter(|a| a.var == var && !a.soft && scope.is_none_or(|b| a.block == b)) {
        s = s.intersect(&a.set);
        used.push(a);
        if s.is_empty() {
            let bounds: Vec<String> = used.iter().map(|a| format!("'{}' ({})", a.text, a.block)).collect();
            out.add(
                "SV-RND-030",
                "ContradictoryConstraints",
                Severity::Error,
                format!(
                    "The constraints on '{var}' in class '{}' leave no legal value: '{ty}' allows {}, and {} together exclude all of it. \
Every randomize() of '{}' fails while these constraints are on.",
                    cc.name,
                    range.show(),
                    bounds.join(" with "),
                    cc.name
                ),
                c.loc(a.tok),
            );
            return true;
        }
    }
    false
}
