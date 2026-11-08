//! SV-RND-031 (SoftConstraintNeverHolds): Flags soft constraints that the
//! class's own hard constraints always override (`soft x == 9` with
//! `x inside {[1:3]}`): the default they express never takes effect.

use super::constraint_sets::{classes, hard_set, uses_constraint_mode};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;

pub(crate) fn soft_never_holds(_: &Design, csts: &[Cst], out: &mut Sink) {
    let mode = uses_constraint_mode(csts);
    for c in csts {
        for cc in classes(c) {
            for a in cc.atoms.iter().filter(|a| a.soft) {
                // With constraint_mode in use, only the soft bound's own block.
                let hard = hard_set(&cc, &a.var, mode.then_some(a.block.as_str()));
                if hard.is_empty() || !hard.intersect(&a.set).is_empty() {
                    continue;
                }
                out.add(
                    "SV-RND-031",
                    "SoftConstraintNeverHolds",
                    Severity::Warning,
                    format!(
                        "Soft constraint '{}' in class '{}' never holds: the hard constraints keep '{}' in {}, so the solver always drops it. \
Change the default, or the hard constraints, so that they agree.",
                        a.text,
                        cc.name,
                        a.var,
                        hard.show()
                    ),
                    c.loc(a.tok),
                );
            }
        }
    }
}
