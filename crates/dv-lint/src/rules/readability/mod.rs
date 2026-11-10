//! The readability rules: the Rust twins of `cpp/src/rules/readability/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::profile::*;
use crate::{Check, Rule, Severity};

mod commented_out_code;
mod complex_inline_property;
mod double_negative_flag;
mod magic_number;
mod non_descriptive_name;
mod positional_flag_literals;
mod redundant_boolean;
mod redundant_comment;
mod ref_argument_never_written;
mod unclear_abbreviation;
mod undocumented_extension_point;
mod undocumented_testbench_class;
mod unparenthesized_macro_argument;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-API-001", name: "RefArgumentNeverWritten", category: "Maintainability", severity: Severity::Info, profiles: SMELLS, guidelines: &["MG-9.1"], experimental: false, native: false, about: "Flags ref arguments that the routine only reads; declare them const ref so callers know they are not modified (clang-tidy readability-non-const-parameter). Virtual methods are skipped because overrides share the signature.", check: Check::Syntax(ref_argument_never_written::ref_argument_never_written),
        },
        Rule { id: "SV-MAC-002", name: "UnparenthesizedMacroArgument", category: "Correctness", severity: Severity::Warning, profiles: SOUNDNESS, guidelines: &[], experimental: false, native: false, about: "Flags expression macros that use an argument next to an operator without parentheses, or whose body is not wrapped in parentheses. `define DOUBLE(x) x*2 turns DOUBLE(a+1) into a+1*2; write `define DOUBLE(x) ((x)*2).", check: Check::Syntax(unparenthesized_macro_argument::unparenthesized_macro),
        },
        Rule { id: "SV-RDB-001", name: "PositionalFlagLiterals", category: "Maintainability", severity: Severity::Info, profiles: SMELLS, guidelines: &["MG-2.4"], experimental: false, native: false, about: "Flags calls that pass two or more 0/1 literals positionally to 1-bit arguments; use named argument binding (clang-tidy bugprone-argument-comment, Clean Code flag arguments).", check: Check::Syntax(positional_flag_literals::positional_flags),
        },
        Rule { id: "SV-RDB-002", name: "MagicNumber", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &["MG-9.1"], experimental: false, native: false, about: "Flags numeric literals other than 0, 1 and -1 in class method bodies: conditions, loop bounds and sizes; delays are left to the magic-delay rules (Checkstyle MagicNumber, Sonar S109). Advisory: off by default.", check: Check::Syntax(magic_number::magic_number) },
        Rule { id: "SV-RDB-003", name: "CommentedOutCode", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &["MG-9.1"], experimental: false, native: false, about: "Flags blocks of // comments that parse as SystemVerilog statements or declarations (Sonar S125). Advisory: off by default.", check: Check::Syntax(commented_out_code::commented_out_code),
        },
        Rule { id: "SV-RDB-004", name: "UndocumentedExtensionPoint", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &["MG-5.2"], experimental: false, native: false, about: "Flags new virtual methods without a preceding comment in classes meant to be extended: classes with user subclasses, classes named *base*, and callbacks (Checkstyle MissingJavadocMethod, Pylint C0116). Advisory: off by default.", check: Check::Mixed(undocumented_extension_point::undocumented_extension_point),
        },
        Rule { id: "SV-RDB-005", name: "RedundantBoolean", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &["MG-9.1"], experimental: false, native: false, about: "Flags 1-bit values compared with 1'b1/1'b0 and 'if (c) return 1; else return 0;' (Sonar S1125, clang-tidy readability-simplify-boolean-expr). Advisory: off by default.", check: Check::Syntax(redundant_boolean::redundant_boolean) },
        Rule { id: "SV-RDB-006", name: "DoubleNegativeFlag", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &[], experimental: false, native: false, about: "Flags '!' (or '~' on one bit) applied to a variable whose name is negative (disable_*, no_*, not_*, skip_*, inhibit_*, ...). Advisory: off by default.", check: Check::Syntax(double_negative_flag::double_negative_flag),
        },
        Rule { id: "SV-RDB-007", name: "NonDescriptiveName", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &[], experimental: false, native: false, about: "Flags class properties, and locals of routines longer than min_lines (default 10), named tmp, temp, val, obj, foo, d, t and the like (with or without a number; locals also data and value). Arguments and loop variables are not checked. Advisory: off by default.", check: Check::Mixed(non_descriptive_name::non_descriptive_name),
        },
        Rule { id: "SV-RDB-008", name: "UnclearAbbreviation", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &[], experimental: false, native: false, about: "Flags class, property, method and local names containing a vowel-less word of four or more letters (trnsfr, chnnl, rgstr) that is not a common DV short form (clk, cfg, ctrl, jtag, ...). Each person drops different vowels, so such names defeat grep. Advisory: off by default.", check: Check::Mixed(unclear_abbreviation::unclear_abbreviation),
        },
        Rule { id: "SV-RDB-009", name: "RedundantComment", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &[], experimental: false, native: false, about: "Flags a one-line // comment directly above a statement when every word of the comment (2 to 6 words, ignoring a, the, to, ...) is already a word of the statement's names, e.g. '// set cfg' above set_cfg(). Advisory: off by default.", check: Check::Syntax(redundant_comment::redundant_comment) },
        Rule { id: "SV-RDB-010", name: "UndocumentedTestbenchClass", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &[], experimental: false, native: false, about: "Flags UVM components and sequences without a comment of at least min_words (default 10) words of prose right above the class. Copyright and license lines do not count. Advisory: off by default.", check: Check::Mixed(undocumented_testbench_class::undocumented_testbench_class),
        },
        Rule { id: "SV-SVA-006", name: "ComplexInlineProperty", category: "Advisory", severity: Severity::Info, profiles: ADVISORY, guidelines: &[], experimental: false, native: false, about: "Flags assert/assume/cover property statements whose inline property uses more than max_temporal_ops (default 3) temporal operators (##, |->, |=>, [*], throughout, within, until, ...). Advisory: off by default.", check: Check::Syntax(complex_inline_property::complex_inline_property),
        },
    ]
}
