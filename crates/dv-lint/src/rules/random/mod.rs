//! The random rules: the Rust twins of `cpp/src/rules/random/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

mod concurrent_randomize_in_fork;
mod constant_reseed;
mod constraint_calling_non_pure_function;
mod constraint_implication_multi_bit;
mod constraint_inside_inverted_range;
mod cyclic_solve_before;
mod dist_wide_range_colon_eq;
mod empty_randomize_with;
mod global_rng_in_class;
mod inline_shadow_no_local;
mod non_invertible_constraint_op;
mod non_rand_field_constrained;
mod randc_in_dynamic_container;
mod randc_memory_explosion;
mod randc_state_exhaustion;
mod randomize_in_constructor;
mod randomize_inside_assert;
mod randomize_missing_super;
mod redundant_constraint;
mod static_constraint;
mod unallocated_rand_object_array;
mod unchecked_randomize;
mod unconstrained_rand_variable;
mod unrelated_solve_before;
mod unrestored_rand_mode;
mod zero_weight_distribution;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-RND-001", name: "UncheckedRandomize", check: Check::Syntax(unchecked_randomize::unchecked_randomize) },
        Rule { id: "SV-RND-002", name: "GlobalRngInClass", check: Check::Model(global_rng_in_class::global_rng_in_class) },
        Rule { id: "SV-RND-003", name: "RandomizeMissingSuper", check: Check::Model(randomize_missing_super::randomize_missing_super) },
        Rule { id: "SV-RND-004", name: "InlineShadowNoLocal", check: Check::Syntax(inline_shadow_no_local::inline_shadow_no_local) },
        Rule { id: "SV-RND-005", name: "NonRandFieldConstrained", check: Check::Syntax(non_rand_field_constrained::non_rand_field_constrained) },
        Rule { id: "SV-RND-006", name: "RandcMemoryExplosion", check: Check::Syntax(randc_memory_explosion::randc_wide) },
        Rule { id: "SV-RND-008", name: "NonInvertibleConstraintOp", check: Check::Syntax(non_invertible_constraint_op::non_invertible_op) },
        Rule { id: "SV-RND-009", name: "StaticConstraint", check: Check::Syntax(static_constraint::static_constraint) },
        Rule { id: "SV-RND-010", name: "CyclicSolveBefore", check: Check::Syntax(cyclic_solve_before::cyclic_solve_before) },
        Rule { id: "SV-RND-011", name: "RedundantConstraint", check: Check::Syntax(redundant_constraint::redundant_constraint) },
        Rule { id: "SV-RND-012", name: "ZeroWeightDistribution", check: Check::Syntax(zero_weight_distribution::zero_weight_dist) },
        Rule { id: "SV-RND-013", name: "UnrestoredRandMode", check: Check::Syntax(unrestored_rand_mode::unrestored_rand_mode) },
        Rule { id: "SV-RND-014", name: "EmptyRandomizeWith", check: Check::Syntax(empty_randomize_with::empty_randomize_with) },
        Rule { id: "SV-RND-015", name: "UnconstrainedRandVariable", check: Check::Syntax(unconstrained_rand_variable::unconstrained_rand) },
        Rule { id: "SV-RND-016", name: "ConcurrentRandomizeInFork", check: Check::Syntax(concurrent_randomize_in_fork::concurrent_randomize) },
        Rule { id: "SV-RND-017", name: "RandcStateExhaustion", check: Check::Syntax(randc_state_exhaustion::randc_narrow) },
        Rule { id: "SV-RND-018", name: "RandomizeInConstructor", check: Check::Model(randomize_in_constructor::randomize_in_constructor) },
        Rule { id: "SV-RND-019", name: "ConstraintImplicationMultiBit", check: Check::Syntax(constraint_implication_multi_bit::implication_multi_bit) },
        Rule { id: "SV-RND-020", name: "RandcInDynamicContainer", check: Check::Syntax(randc_in_dynamic_container::randc_dynamic) },
        Rule { id: "SV-RND-021", name: "ConstraintInsideInvertedRange", check: Check::Syntax(constraint_inside_inverted_range::inverted_range) },
        Rule { id: "SV-RND-022", name: "ConstraintCallingNonPureFunction", check: Check::Syntax(constraint_calling_non_pure_function::non_pure_in_constraint) },
        Rule { id: "SV-RND-023", name: "UnallocatedRandObjectArray", check: Check::Syntax(unallocated_rand_object_array::unallocated_rand_array) },
        Rule { id: "SV-RND-024", name: "RandomizeInsideAssert", check: Check::Syntax(randomize_inside_assert::randomize_in_assert) },
        Rule { id: "SV-RND-025", name: "DistWideRangeColonEq", check: Check::Syntax(dist_wide_range_colon_eq::dist_wide_range) },
        Rule { id: "SV-RND-026", name: "UnrelatedSolveBefore", check: Check::Syntax(unrelated_solve_before::unrelated_solve_before) },
        Rule { id: "SV-TST-001", name: "ConstantReseed", check: Check::Syntax(constant_reseed::constant_reseed) },
    ]
}
