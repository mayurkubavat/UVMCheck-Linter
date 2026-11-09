//! dv-lint's own analyzers (IEEE plan S4): what a compiler checks but never
//! computes, because it belongs to the simulator. Constraints are
//! type-checked but never solved (`contradictory_constraints`,
//! `soft_constraint_never_holds`, `dead_dist_item`), covergroup bins are
//! never counted (`cross_bin_count`), processes are never scheduled
//! (`clocked_blocking_race`), the C side of DPI is never seen
//! (`conflicting_dpi_prototypes`), and where transactions go for coverage is
//! a testbench-wide question (`uncovered_analysis_port`). None has a uvmcheck twin: each is in
//! [`crate::NATIVE_ONLY`].

use crate::{Check, Rule};

mod clocked_blocking_race;
mod conflicting_dpi_prototypes;
mod constraint_sets;
mod contradictory_constraints;
mod cross_bin_count;
mod dead_dist_item;
mod soft_constraint_never_holds;
mod uncovered_analysis_port;
mod values;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule {
            id: "SV-RND-030",
            name: "ContradictoryConstraints",
            check: Check::Mixed(contradictory_constraints::contradictory_constraints),
        },
        Rule {
            id: "SV-RND-031",
            name: "SoftConstraintNeverHolds",
            check: Check::Mixed(soft_constraint_never_holds::soft_never_holds),
        },
        Rule { id: "SV-RND-032", name: "DeadDistItem", check: Check::Mixed(dead_dist_item::dead_dist_item) },
        Rule {
            id: "SV-COV-018",
            name: "CrossBinCountTooLarge",
            check: Check::Syntax(cross_bin_count::cross_bin_count),
        },
        Rule {
            id: "SV-RCE-002",
            name: "ClockedBlockingRace",
            check: Check::Syntax(clocked_blocking_race::clocked_blocking_race),
        },
        Rule {
            id: "UVM-TLM-007",
            name: "UncoveredAnalysisPort",
            check: Check::Mixed(uncovered_analysis_port::uncovered_analysis_ports),
        },
        Rule {
            id: "DPI-PRT-001",
            name: "ConflictingDpiPrototypes",
            check: Check::Mixed(conflicting_dpi_prototypes::conflicting_dpi_prototypes),
        },
    ]
}
