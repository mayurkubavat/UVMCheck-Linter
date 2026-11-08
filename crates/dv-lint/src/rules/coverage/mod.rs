//! The coverage rules: the Rust twins of `cpp/src/rules/coverage/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

mod at_least_zero_or_negative;
mod covergroup_cross_bin_explosion;
mod coverpoint_default_bin_hazard;
mod coverpoint_weight_zero;
mod cross_same_coverpoint;
mod cross_with_all_bins_ignored;
mod cross_with_empty_bins;
mod empty_covergroup;
mod ignore_bins_overlaps_illegal_bins;
mod illegal_bin_overlap;
mod missing_iff_on_covergroup_clock;
mod overlapping_ranges_in_coverpoint_bin;
mod unbounded_transition_bins;
mod unreachable_cover_bin;
mod unreferenced_covergroup;
mod wide_coverpoint_without_bins;
mod zero_weight_coverpoint;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-COV-001", name: "UnreferencedCovergroup", check: Check::Syntax(unreferenced_covergroup::unreferenced_covergroup) },
        Rule { id: "SV-COV-002", name: "ZeroWeightCoverpoint", check: Check::Syntax(zero_weight_coverpoint::zero_weight) },
        Rule { id: "SV-COV-003", name: "CrossSameCoverpoint", check: Check::Syntax(cross_same_coverpoint::cross_same_coverpoint) },
        Rule { id: "SV-COV-004", name: "IllegalBinOverlap", check: Check::Syntax(illegal_bin_overlap::illegal_bin_overlap) },
        Rule { id: "SV-COV-005", name: "UnboundedTransitionBins", check: Check::Syntax(unbounded_transition_bins::unbounded_transition) },
        Rule { id: "SV-COV-006", name: "CovergroupCrossBinExplosion", check: Check::Syntax(covergroup_cross_bin_explosion::cross_explosion) },
        Rule { id: "SV-COV-007", name: "EmptyCovergroup", check: Check::Syntax(empty_covergroup::empty_covergroup) },
        Rule { id: "SV-COV-008", name: "CrossWithEmptyBins", check: Check::Syntax(cross_with_empty_bins::cross_with_empty_bins) },
        Rule { id: "SV-COV-009", name: "CoverpointDefaultBinHazard", check: Check::Syntax(coverpoint_default_bin_hazard::default_bin) },
        Rule { id: "SV-COV-010", name: "CrossWithAllBinsIgnored", check: Check::Syntax(cross_with_all_bins_ignored::cross_all_ignored) },
        Rule { id: "SV-COV-011", name: "AtLeastZeroOrNegative", check: Check::Syntax(at_least_zero_or_negative::at_least_zero) },
        Rule { id: "SV-COV-012", name: "WideCoverpointWithoutBins", check: Check::Syntax(wide_coverpoint_without_bins::coverpoint_without_bins) },
        Rule { id: "SV-COV-013", name: "IgnoreBinsOverlapsIllegalBins", check: Check::Syntax(ignore_bins_overlaps_illegal_bins::ignore_illegal_overlap) },
        Rule { id: "SV-COV-014", name: "CoverpointWeightZero", check: Check::Syntax(coverpoint_weight_zero::weight_zero) },
        Rule { id: "SV-COV-015", name: "MissingIffOnCovergroupClock", check: Check::Syntax(missing_iff_on_covergroup_clock::missing_iff) },
        Rule { id: "SV-COV-016", name: "OverlappingRangesInCoverpointBin", check: Check::Syntax(overlapping_ranges_in_coverpoint_bin::overlapping_ranges) },
        Rule { id: "SV-COV-017", name: "UnreachableCoverBin", check: Check::Syntax(unreachable_cover_bin::unreachable_bin) },
    ]
}
