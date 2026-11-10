//! The sva rules: the Rust twins of `cpp/src/rules/sva/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::profile::*;
use crate::{Check, Rule, Severity};

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
        Rule { id: "SV-SVA-001", name: "ImmediateAssertInCombinatorialLogic", category: "SVA", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Simple immediate assertions in always_comb or combinatorial blocks evaluate transient delta glitches. Convert to deferred immediate assertions (assert #0 or assert final) to wait for settled circuit values.", check: Check::Syntax(immediate_assert_in_combinatorial_logic::immediate_in_comb),
        },
        Rule { id: "SV-SVA-002", name: "DisableIffComplexExpression", category: "SVA", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags 'disable iff (...)' clauses containing complex boolean expressions or multi-variable logic instead of simple reset conditions.", check: Check::Syntax(disable_iff_complex_expression::disable_iff_complex),
        },
        Rule { id: "SV-SVA-003", name: "MixedClockEdgesInProperty", category: "SVA", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags SVA properties or assertions that mix both 'posedge' and 'negedge' clocking events.", check: Check::Syntax(mixed_clock_edges_in_property::mixed_edges),
        },
        Rule { id: "SV-SVA-004", name: "NonConsecutiveRepetitionZeroBound", category: "SVA", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Non-consecutive repetition operators ([= ...] and [-> ...]) must specify a minimum bound >= 1.", check: Check::Syntax(non_consecutive_repetition_zero_bound::zero_repetition),
        },
        Rule { id: "SV-SVA-005", name: "MultiClockConcurrentAssertionDomain", category: "SVA", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows concurrent assertions mixing multiple asynchronous clock domains without formal CDC qualification.", check: Check::Syntax(multi_clock_concurrent_assertion_domain::multi_clock),
        },
        Rule { id: "SVA-ACT-001", name: "AssertWithoutElse", category: "SVA", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &["MG-8.1"], experimental: false, native: false, about: "Immediate and concurrent assertions must include an 'else' action block (e.g. 'else `uvm_error(...)'). Assertions without action blocks produce unhelpful one-line messages and cannot integrate with UVM reporting.", check: Check::Syntax(assert_without_else::assert_without_else),
        },
        Rule { id: "SVA-ACT-002", name: "TimeInAssertionAction", category: "SVA", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Action blocks of immediate and concurrent assertions must not consume simulation time (# delays, @ event controls, or wait statements) per IEEE 1800 § 16.", check: Check::Syntax(time_in_assertion_action::time_in_action),
        },
        Rule { id: "SVA-ACT-003", name: "SideEffectInAssertionAction", category: "SVA", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags assignments, increments and other writes inside the pass or fail action of an immediate or concurrent assertion. Reporting calls are fine; state changes vanish when assertions are disabled.", check: Check::Syntax(side_effect_in_assertion_action::side_effect_in_action),
        },
        Rule { id: "SVA-CTX-001", name: "ConcurrentAssertInFunction", category: "SVA", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallow concurrent assertions ('assert/assume/cover property') inside functions. Functions execute in zero simulation time and lack clocking contexts required by SVA semantics.", check: Check::Syntax(concurrent_assert_in_function::concurrent_in_function),
        },
        Rule { id: "SVA-CYC-001", name: "SvaUnboundedDelay", category: "SVA", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallow unbounded cycle delays (##[1:$]) in SVA sequences. Unbounded delays cause simulator memory thread leaks and formal verification state-space explosions.", check: Check::Syntax(sva_unbounded_delay::unbounded_delay),
        },
        Rule { id: "SVA-LBL-001", name: "UnlabeledAssertion", category: "SVA", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Immediate and concurrent assertions must declare an explicit label prefix (e.g. 'check_lbl: assert ...'). Lacking labels makes regression failure triage, ticket routing, and assertion control commands difficult.", check: Check::Syntax(unlabeled_assertion::unlabeled_assertion),
        },
        Rule { id: "SVA-RST-001", name: "DisableIffReset", category: "SVA", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Concurrent assertions and property declarations must include a 'disable iff (!rst_n)' clause to gate checking during power-on reset and warm resets, avoiding false-failure storms.", check: Check::Syntax(disable_iff_reset::disable_iff_reset) },
        Rule { id: "SVA-VAC-001", name: "SvaVacuousAntecedent", category: "SVA", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Detects constant-false antecedents in assertion implications (0 |-> ...). False antecedents cause vacuous passes, giving a false 100% pass metric while never verifying DUT behavior.", check: Check::Syntax(sva_vacuous_antecedent::vacuous_antecedent),
        },
    ]
}
