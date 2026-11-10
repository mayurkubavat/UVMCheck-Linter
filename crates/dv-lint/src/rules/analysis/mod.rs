//! dv-lint's own analyzers (IEEE plan S4): what a compiler checks but never
//! computes, because it belongs to the simulator. Constraints are
//! type-checked but never solved (`contradictory_constraints`,
//! `soft_constraint_never_holds`, `dead_dist_item`), covergroup bins are
//! never counted (`cross_bin_count`), processes are never scheduled
//! (`clocked_blocking_race`), the C side of DPI is never seen
//! (`conflicting_dpi_prototypes`), and whether coverage is ever collected
//! is a testbench-wide question (`uncovered_analysis_port`,
//! `covergroup_never_sampled`). None has a uvmcheck twin: each is in
//! [`crate::NATIVE_ONLY`].

use crate::profile::*;
use crate::{Check, Rule, Severity};

mod clocked_blocking_race;
mod conflicting_dpi_prototypes;
mod constraint_sets;
mod contradictory_constraints;
mod covergroup_never_sampled;
mod cross_bin_count;
mod dead_dist_item;
mod soft_constraint_never_holds;
mod uncovered_analysis_port;
mod values;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-RND-030", name: "ContradictoryConstraints", category: "Randomization", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: true, about: "A rand variable whose hard constraints leave no legal value (`len > 10` on a `bit [3:0]`; `x < 5` with `x inside {[8:9]}`). Bounds in different constraint blocks count together only when no user code calls `constraint_mode()`.", check: Check::Mixed(contradictory_constraints::contradictory_constraints),
        },
        Rule { id: "SV-RND-031", name: "SoftConstraintNeverHolds", category: "Randomization", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: true, about: "A `soft` constraint the class's hard constraints always override.", check: Check::Mixed(soft_constraint_never_holds::soft_never_holds),
        },
        Rule { id: "SV-RND-032", name: "DeadDistItem", category: "Randomization", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: true, about: "A `dist` item whose values the hard constraints or the type never allow, so its weight is dropped.", check: Check::Mixed(dead_dist_item::dead_dist_item) },
        Rule { id: "SV-COV-018", name: "CrossBinCountTooLarge", category: "Coverage", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: true, about: "A cross whose bin count, computed as a simulator builds the bins (explicit, `[N]`, `[]`, automatic up to `auto_bin_max`), is above 10,000.", check: Check::Syntax(cross_bin_count::cross_bin_count),
        },
        Rule { id: "SV-RCE-002", name: "ClockedBlockingRace", category: "Concurrency", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: true, about: "A variable written with `=` in one edge-triggered always block and read by another on the same edge: a race (IEEE 1800 4.7).", check: Check::Syntax(clocked_blocking_race::clocked_blocking_race),
        },
        Rule { id: "SV-COV-019", name: "CovergroupNeverSampled", category: "Coverage", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: true, about: "A covergroup declared in a module, interface, program or package with no sampling event that nothing instantiates, or whose instances no `.sample()` call anywhere in the design targets (`cg_i.sample()`, `bus.cg_i.sample()`, `cg_q[i].sample(x)`). The class covergroups are SV-COV-001's. Quiet on the corpus, whose module and interface covergroups all have a clocking event.", check: Check::Mixed(covergroup_never_sampled::covergroup_never_sampled),
        },
        Rule { id: "UVM-TLM-007", name: "UncoveredAnalysisPort", category: "UVM", severity: Severity::Warning, profiles: METHODOLOGY, guidelines: &[], experimental: false, native: true, about: "A transaction type a monitor publishes on an analysis port that no coverage collector receives: no class with a covergroup (its own, a base class's or a held object's) extends `uvm_subscriber #(T)` or holds an analysis imp, export, FIFO or get/peek port of `T`. Only reported when the testbench declares covergroups at all; one finding per type, naming every port. On the corpus it finds core-v-verif's agents, whose coverage models receive the monitor transactions but have only TODO covergroups.", check: Check::Mixed(uncovered_analysis_port::uncovered_analysis_ports),
        },
        Rule { id: "DPI-PRT-001", name: "ConflictingDpiPrototypes", category: "DPI", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: true, about: "One C function imported with two different prototypes.", check: Check::Mixed(conflicting_dpi_prototypes::conflicting_dpi_prototypes),
        },
    ]
}
