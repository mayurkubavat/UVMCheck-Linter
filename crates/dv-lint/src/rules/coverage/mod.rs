//! The coverage rules: the Rust twins of `cpp/src/rules/coverage/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::profile::*;
use crate::{Check, Rule, Severity};

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
        Rule { id: "SV-COV-001", name: "UnreferencedCovergroup", category: "Coverage", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Detects covergroups declared in classes or modules that are never instantiated with 'new()' or never sampled. Unreferenced covergroups contribute 0 coverage samples, creating silent verification blind spots.", check: Check::Syntax(unreferenced_covergroup::unreferenced_covergroup),
        },
        Rule { id: "SV-COV-002", name: "ZeroWeightCoverpoint", category: "Coverage", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags covergroups, coverpoints, or crosses configured with option.weight or type_option.weight set to 0. Zero-weight coverage constructs collect samples but contribute 0% to regression coverage closure.", check: Check::Syntax(zero_weight_coverpoint::zero_weight),
        },
        Rule { id: "SV-COV-003", name: "CrossSameCoverpoint", category: "Coverage", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Detects coverage cross constructs crossing a coverpoint with itself (cross cp_a, cp_a). Crossing a coverpoint with itself creates redundant diagonal bins that distort coverage calculation.", check: Check::Syntax(cross_same_coverpoint::cross_same_coverpoint),
        },
        Rule { id: "SV-COV-004", name: "IllegalBinOverlap", category: "Coverage", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Detects overlapping values between regular 'bins' and 'illegal_bins' or 'ignore_bins' within a coverpoint. Per IEEE 1800 § 19.5, regular bins must not overlap with illegal or ignore bins.", check: Check::Syntax(illegal_bin_overlap::illegal_bin_overlap),
        },
        Rule { id: "SV-COV-005", name: "UnboundedTransitionBins", category: "Coverage", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Transition sequences in coverpoint bins must not specify unbounded repetitions (e.g., [*1:$], [->1:$]). Unbounded transition sequences cause exponential memory growth in simulator coverage databases.", check: Check::Syntax(unbounded_transition_bins::unbounded_transition),
        },
        Rule { id: "SV-COV-006", name: "CovergroupCrossBinExplosion", category: "Coverage", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows crossing 3 or more coverpoints without 'ignore_bins' or 'binsof' exclusion filters. Unfiltered multi-way crosses cause combinatorial auto-bin explosion, leading to simulator memory exhaustion.", check: Check::Syntax(covergroup_cross_bin_explosion::cross_explosion),
        },
        Rule { id: "SV-COV-007", name: "EmptyCovergroup", category: "Coverage", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags covergroups declared with zero coverpoints and zero crosses. An empty covergroup collects no coverage data.", check: Check::Syntax(empty_covergroup::empty_covergroup) },
        Rule { id: "SV-COV-008", name: "CrossWithEmptyBins", category: "Coverage", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Detects degenerate covergroup cross constructs specifying fewer than 2 coverpoints or empty cross products.", check: Check::Syntax(cross_with_empty_bins::cross_with_empty_bins),
        },
        Rule { id: "SV-COV-009", name: "CoverpointDefaultBinHazard", category: "Coverage", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags non-ignoring 'default' bins in coverpoints that mask uncovered stimulus and give a false sense of coverage closure.", check: Check::Syntax(coverpoint_default_bin_hazard::default_bin),
        },
        Rule { id: "SV-COV-010", name: "CrossWithAllBinsIgnored", category: "Coverage", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags covergroup cross constructs where all defined bins are ignore_bins, producing vacuous 0-bin coverage metrics.", check: Check::Syntax(cross_with_all_bins_ignored::cross_all_ignored),
        },
        Rule { id: "SV-COV-011", name: "AtLeastZeroOrNegative", category: "Coverage", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags option.at_least or type_option.at_least set to 0 or negative values in covergroups/coverpoints. IEEE 1800 § 19.5 requires at_least to be a positive integer >= 1.", check: Check::Syntax(at_least_zero_or_negative::at_least_zero),
        },
        Rule { id: "SV-COV-012", name: "WideCoverpointWithoutBins", category: "Coverage", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags coverpoints that define no explicit bins and omit option.auto_bin_max. Default auto-binning creates 64 bins, producing misleading 100% coverage closure on wide vectors.", check: Check::Syntax(wide_coverpoint_without_bins::coverpoint_without_bins),
        },
        Rule { id: "SV-COV-013", name: "IgnoreBinsOverlapsIllegalBins", category: "Coverage", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags coverage models where the same value or range is specified in both 'ignore_bins' and 'illegal_bins' within a coverpoint.", check: Check::Syntax(ignore_bins_overlaps_illegal_bins::ignore_illegal_overlap),
        },
        Rule { id: "SV-COV-014", name: "CoverpointWeightZero", category: "Coverage", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags covergroups or coverpoints where 'option.weight' or 'type_option.weight' is set to 0, silently excluding it from coverage scores.", check: Check::Syntax(coverpoint_weight_zero::weight_zero),
        },
        Rule { id: "SV-COV-015", name: "MissingIffOnCovergroupClock", category: "Coverage", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Covergroups sampled by a clocking event should specify an 'iff' reset guard condition.", check: Check::Syntax(missing_iff_on_covergroup_clock::missing_iff),
        },
        Rule { id: "SV-COV-016", name: "OverlappingRangesInCoverpointBin", category: "Coverage", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags coverpoint bins containing overlapping numerical value ranges.", check: Check::Syntax(overlapping_ranges_in_coverpoint_bin::overlapping_ranges),
        },
        Rule { id: "SV-COV-017", name: "UnreachableCoverBin", category: "Coverage", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags coverpoint bins whose values all lie outside the range of the coverpoint expression (for example bins {[16:31]} on a 4-bit value). Such a bin can never be hit.", check: Check::Syntax(unreachable_cover_bin::unreachable_bin),
        },
    ]
}
