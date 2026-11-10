//! The random rules: the Rust twins of `cpp/src/rules/random/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::profile::*;
use crate::{Check, Rule, Severity};

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
        Rule { id: "SV-RND-001", name: "UncheckedRandomize", category: "Randomization", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &["MG-8.1"], experimental: false, native: false, about: "The return value of randomize() must be checked. Silently ignoring failure leaves transactions un-randomized or corrupt, concealing verification testbench bugs.", check: Check::Syntax(unchecked_randomize::unchecked_randomize),
        },
        Rule { id: "SV-RND-002", name: "GlobalRngInClass", category: "Randomization", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Calling $urandom or $random inside class components or sequences corrupts global thread random stability. Use std::randomize() or object-scoped randomization to preserve simulation repeatability.", check: Check::Model(global_rng_in_class::global_rng_in_class),
        },
        Rule { id: "SV-RND-003", name: "RandomizeMissingSuper", category: "Randomization", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Overridden pre_randomize() and post_randomize() callback methods must invoke super.pre_randomize() or super.post_randomize() to prevent bypassing base-class transaction setup and validation routines.", check: Check::Model(randomize_missing_super::randomize_missing_super),
        },
        Rule { id: "SV-RND-004", name: "InlineShadowNoLocal", category: "Randomization", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Detects inline constraint variable shadowing inside 'randomize() with' missing 'local::'. Unqualified identifiers resolve to target object properties, turning constraints into no-op tautologies.", check: Check::Syntax(inline_shadow_no_local::inline_shadow_no_local),
        },
        Rule { id: "SV-RND-005", name: "NonRandFieldConstrained", category: "Randomization", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Detects class member fields that are constrained in a constraint block but not declared 'rand' or 'randc'. The solver treats non-rand fields as fixed constants and will never randomize them.", check: Check::Syntax(non_rand_field_constrained::non_rand_field_constrained),
        },
        Rule { id: "SV-RND-006", name: "RandcMemoryExplosion", category: "Randomization", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallow wide vector declarations with 'randc' (> 16 bits). Cyclic permutation tracking across large state spaces causes simulator heap exhaustion and solver startup deadlocks.", check: Check::Syntax(randc_memory_explosion::randc_wide),
        },
        Rule { id: "SV-RND-008", name: "NonInvertibleConstraintOp", category: "Randomization", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallow non-invertible operators (%, /, **) inside constraint blocks. Non-linear operators prevent SAT/SMT solvers from bi-directionally pruning solution spaces, causing catastrophic backtracking and long simulation stalls.", check: Check::Syntax(non_invertible_constraint_op::non_invertible_op),
        },
        Rule { id: "SV-RND-009", name: "StaticConstraint", category: "Randomization", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows static constraints. Static constraints share solver state across all class instances in the simulation, causing hidden dependencies and test flakiness.", check: Check::Syntax(static_constraint::static_constraint) },
        Rule { id: "SV-RND-010", name: "CyclicSolveBefore", category: "Randomization", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Detects cyclic dependencies in solve...before constraint ordering (e.g. solve a before b and solve b before a). Cyclic solve-before constraints create an unsatisfiable dependency graph in the solver.", check: Check::Syntax(cyclic_solve_before::cyclic_solve_before),
        },
        Rule { id: "SV-RND-011", name: "RedundantConstraint", category: "Randomization", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &["MG-7.1"], experimental: false, native: false, about: "Detects duplicate or redundant constraint expressions within the same class scope. Redundant constraints enlarge solver expression graphs and increase solver solve latency without adding constraints.", check: Check::Syntax(redundant_constraint::redundant_constraint),
        },
        Rule { id: "SV-RND-012", name: "ZeroWeightDistribution", category: "Randomization", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Detects constraint distribution blocks where all weights evaluate to zero. At least one weight in a distribution must evaluate to non-zero per IEEE 1800 § 18.5.4.", check: Check::Syntax(zero_weight_distribution::zero_weight_dist),
        },
        Rule { id: "SV-RND-013", name: "UnrestoredRandMode", category: "Randomization", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags '.rand_mode(0)' or '.constraint_mode(0)' calls that are never restored with '1' in the routine. Unrestored disabled random modes permanently corrupt stimulus generation for subsequent transactions.", check: Check::Syntax(unrestored_rand_mode::unrestored_rand_mode),
        },
        Rule { id: "SV-RND-014", name: "EmptyRandomizeWith", category: "Randomization", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags 'randomize() with {}' containing an empty inline constraint block. Empty inline constraint blocks incur dynamic solver scope allocation overhead while adding no constraints.", check: Check::Syntax(empty_randomize_with::empty_randomize_with),
        },
        Rule { id: "SV-RND-015", name: "UnconstrainedRandVariable", category: "Randomization", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags non-static 'rand' class properties that are never constrained in any constraint block within the class.", check: Check::Syntax(unconstrained_rand_variable::unconstrained_rand),
        },
        Rule { id: "SV-RND-016", name: "ConcurrentRandomizeInFork", category: "Randomization", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows concurrent calls to .randomize() on the same object handle across parallel fork threads without synchronization.", check: Check::Syntax(concurrent_randomize_in_fork::concurrent_randomize),
        },
        Rule { id: "SV-RND-017", name: "RandcStateExhaustion", category: "Randomization", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags 'randc' cyclic variables declared with bit-width <= 2 bits (1 or 2 bits). State permutation tables for <= 4 states reset almost immediately, incurring solver overhead with no benefit.", check: Check::Syntax(randc_state_exhaustion::randc_narrow),
        },
        Rule { id: "SV-RND-018", name: "RandomizeInConstructor", category: "Randomization", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows calling randomize() inside class constructors (new()). Randomizing during construction breaks derived-class constraint inheritance and factory overrides.", check: Check::Model(randomize_in_constructor::randomize_in_constructor),
        },
        Rule { id: "SV-RND-019", name: "ConstraintImplicationMultiBit", category: "Randomization", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags constraint implication expressions 'A -> B' where condition 'A' is a multi-bit vector without an explicit boolean comparison.", check: Check::Syntax(constraint_implication_multi_bit::implication_multi_bit),
        },
        Rule { id: "SV-RND-020", name: "RandcInDynamicContainer", category: "Randomization", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows declaring 'randc' cyclic variables on dynamic arrays, queues, or associative arrays (IEEE 1800 § 18.4.1).", check: Check::Syntax(randc_in_dynamic_container::randc_dynamic),
        },
        Rule { id: "SV-RND-021", name: "ConstraintInsideInvertedRange", category: "Randomization", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Disallows inverted value ranges [low : high] where low > high in set membership expressions.", check: Check::Syntax(constraint_inside_inverted_range::inverted_range),
        },
        Rule { id: "SV-RND-022", name: "ConstraintCallingNonPureFunction", category: "Randomization", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Functions called in constraints must be pure and must not have side effects or void return types.", check: Check::Syntax(constraint_calling_non_pure_function::non_pure_in_constraint),
        },
        Rule { id: "SV-RND-023", name: "UnallocatedRandObjectArray", category: "Randomization", severity: Severity::Error, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags rand arrays of class handles whose elements are never created. randomize() sizes a dynamic array but does not construct objects, so every element stays null and its rand members are never randomized.", check: Check::Syntax(unallocated_rand_object_array::unallocated_rand_array),
        },
        Rule { id: "SV-RND-024", name: "RandomizeInsideAssert", category: "Randomization", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags randomize() used as the condition of an immediate assert or assume. Turning assertions off also skips the randomization, so the object silently keeps stale values.", check: Check::Syntax(randomize_inside_assert::randomize_in_assert),
        },
        Rule { id: "SV-RND-025", name: "DistWideRangeColonEq", category: "Randomization", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags '[lo:hi] := w' in a dist when the range holds more than max_range_values (default 16) values and the dist also has items of another size. ':=' applies the weight to each value, which is rarely what the author meant; ':/' shares it.", check: Check::Syntax(dist_wide_range_colon_eq::dist_wide_range),
        },
        Rule { id: "SV-RND-026", name: "UnrelatedSolveBefore", category: "Randomization", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags 'solve a before b' when no constraint of the class (or its base classes) ties a to b, directly or through other variables. The ordering then has no effect except to suggest a dependency that does not exist.", check: Check::Syntax(unrelated_solve_before::unrelated_solve_before),
        },
        Rule { id: "SV-TST-001", name: "ConstantReseed", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &[], experimental: false, native: false, about: "Flags srandom() or set_randstate() with a constant in components, sequences and other non-test classes; every seed of the regression then runs the same stimulus. Advisory: off by default.", check: Check::Syntax(constant_reseed::constant_reseed) },
    ]
}
