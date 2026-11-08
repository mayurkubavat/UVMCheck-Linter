//! The readability rules: the Rust twins of `cpp/src/rules/readability/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

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
        Rule { id: "SV-API-001", name: "RefArgumentNeverWritten", check: Check::Syntax(ref_argument_never_written::ref_argument_never_written) },
        Rule { id: "SV-MAC-002", name: "UnparenthesizedMacroArgument", check: Check::Syntax(unparenthesized_macro_argument::unparenthesized_macro) },
        Rule { id: "SV-RDB-001", name: "PositionalFlagLiterals", check: Check::Syntax(positional_flag_literals::positional_flags) },
        Rule { id: "SV-RDB-002", name: "MagicNumber", check: Check::Syntax(magic_number::magic_number) },
        Rule { id: "SV-RDB-003", name: "CommentedOutCode", check: Check::Syntax(commented_out_code::commented_out_code) },
        Rule { id: "SV-RDB-004", name: "UndocumentedExtensionPoint", check: Check::Mixed(undocumented_extension_point::undocumented_extension_point) },
        Rule { id: "SV-RDB-005", name: "RedundantBoolean", check: Check::Syntax(redundant_boolean::redundant_boolean) },
        Rule { id: "SV-RDB-006", name: "DoubleNegativeFlag", check: Check::Syntax(double_negative_flag::double_negative_flag) },
        Rule { id: "SV-RDB-007", name: "NonDescriptiveName", check: Check::Mixed(non_descriptive_name::non_descriptive_name) },
        Rule { id: "SV-RDB-008", name: "UnclearAbbreviation", check: Check::Mixed(unclear_abbreviation::unclear_abbreviation) },
        Rule { id: "SV-RDB-009", name: "RedundantComment", check: Check::Syntax(redundant_comment::redundant_comment) },
        Rule { id: "SV-RDB-010", name: "UndocumentedTestbenchClass", check: Check::Mixed(undocumented_testbench_class::undocumented_testbench_class) },
        Rule { id: "SV-SVA-006", name: "ComplexInlineProperty", check: Check::Syntax(complex_inline_property::complex_inline_property) },
    ]
}
