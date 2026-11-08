//! SV-RND-032 (DeadDistItem): Flags dist items whose values the class's
//! hard constraints (or the variable's type) never allow: their weight is
//! silently dropped, so the distribution is not the one written.

use super::constraint_sets::{classes, hard_set, uses_constraint_mode};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;

pub(crate) fn dead_dist_item(_: &Design, csts: &[Cst], out: &mut Sink) {
    let mode = uses_constraint_mode(csts);
    for c in csts {
        for cc in classes(c) {
            for d in &cc.dists {
                let hard = hard_set(&cc, &d.var, mode.then_some(d.block.as_str()));
                if hard.is_empty() {
                    continue;
                }
                for (set, text, tok) in &d.items {
                    if !hard.intersect(set).is_empty() {
                        continue;
                    }
                    out.add(
                        "SV-RND-032",
                        "DeadDistItem",
                        Severity::Warning,
                        format!(
                            "dist item '{text}' on '{}' in class '{}' can never be chosen: '{}' and the hard constraints allow only {}. \
Its weight is dropped, so the other items' share is not the one written.",
                            d.var,
                            cc.name,
                            cc.vars[&d.var].1,
                            hard.show()
                        ),
                        c.loc(*tok),
                    );
                }
            }
        }
    }
}
