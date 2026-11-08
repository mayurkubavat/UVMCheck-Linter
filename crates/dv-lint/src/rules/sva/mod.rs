//! The sva rules: the Rust twins of `cpp/src/rules/sva/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

mod assert_without_else;
mod concurrent_assert_in_function;
mod disable_iff_complex_expression;
mod disable_iff_reset;
mod immediate_assert_in_combinatorial_logic;
mod mixed_clock_edges_in_property;
mod multi_clock_concurrent_assertion_domain;
mod non_consecutive_repetition_zero_bound;
mod side_effect_in_assertion_action;
mod sva_unbounded_delay;
mod sva_vacuous_antecedent;
mod time_in_assertion_action;
mod unlabeled_assertion;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule {
            id: "SV-SVA-001",
            name: "ImmediateAssertInCombinatorialLogic",
            check: Check::Syntax(immediate_assert_in_combinatorial_logic::immediate_in_comb),
        },
        Rule {
            id: "SV-SVA-002",
            name: "DisableIffComplexExpression",
            check: Check::Syntax(disable_iff_complex_expression::disable_iff_complex),
        },
        Rule {
            id: "SV-SVA-003",
            name: "MixedClockEdgesInProperty",
            check: Check::Syntax(mixed_clock_edges_in_property::mixed_edges),
        },
        Rule {
            id: "SV-SVA-004",
            name: "NonConsecutiveRepetitionZeroBound",
            check: Check::Syntax(non_consecutive_repetition_zero_bound::zero_repetition),
        },
        Rule {
            id: "SV-SVA-005",
            name: "MultiClockConcurrentAssertionDomain",
            check: Check::Syntax(multi_clock_concurrent_assertion_domain::multi_clock),
        },
        Rule {
            id: "SVA-ACT-001",
            name: "AssertWithoutElse",
            check: Check::Syntax(assert_without_else::assert_without_else),
        },
        Rule {
            id: "SVA-ACT-002",
            name: "TimeInAssertionAction",
            check: Check::Syntax(time_in_assertion_action::time_in_action),
        },
        Rule {
            id: "SVA-ACT-003",
            name: "SideEffectInAssertionAction",
            check: Check::Syntax(side_effect_in_assertion_action::side_effect_in_action),
        },
        Rule {
            id: "SVA-CTX-001",
            name: "ConcurrentAssertInFunction",
            check: Check::Syntax(concurrent_assert_in_function::concurrent_in_function),
        },
        Rule {
            id: "SVA-CYC-001",
            name: "SvaUnboundedDelay",
            check: Check::Syntax(sva_unbounded_delay::unbounded_delay),
        },
        Rule {
            id: "SVA-LBL-001",
            name: "UnlabeledAssertion",
            check: Check::Syntax(unlabeled_assertion::unlabeled_assertion),
        },
        Rule { id: "SVA-RST-001", name: "DisableIffReset", check: Check::Syntax(disable_iff_reset::disable_iff_reset) },
        Rule {
            id: "SVA-VAC-001",
            name: "SvaVacuousAntecedent",
            check: Check::Syntax(sva_vacuous_antecedent::vacuous_antecedent),
        },
    ]
}
