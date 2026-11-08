//! The style rules: the Rust twins of `cpp/src/rules/style/`, one file per rule,
//! tested by `testdata/rules/<area>/<ID>/`.

use crate::{Check, Rule};

mod class_member_prefix;
mod configurable_naming_engine;
mod configurable_style_engine;
mod constraint_name_suffix;
mod explicit_literal_width;
mod extern_method_threshold;
mod include_guard;
mod interface_naming_convention;
mod macro_trailing_semicolon;
mod mandatory_matching_end_label;
mod max_line_length;
mod method_banner_style;
mod mismatched_end_label;
mod one_statement_per_line;
mod package_naming_convention;
mod section_header_banner;
mod space_keyword_paren;
mod todo_comment_tracker;
mod trailing_whitespace;
mod typedef_name_suffix;
mod uvm_class_type_suffix;
mod uvm_component_naming_convention;
mod uvm_create_string_mismatch;
mod uvm_tlm_port_suffix;

/// Every rule of this category.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule {
            id: "SV-NAM-001",
            name: "UvmComponentNamingConvention",
            check: Check::Model(uvm_component_naming_convention::uvm_component_naming_convention),
        },
        Rule {
            id: "SV-NAM-002",
            name: "PackageNamingConvention",
            check: Check::Syntax(package_naming_convention::package_naming),
        },
        Rule {
            id: "SV-NAM-003",
            name: "InterfaceNamingConvention",
            check: Check::Syntax(interface_naming_convention::interface_naming),
        },
        Rule {
            id: "SV-NAM-005",
            name: "UvmClassTypeSuffix",
            check: Check::Syntax(uvm_class_type_suffix::uvm_class_suffix),
        },
        Rule {
            id: "SV-NAM-100",
            name: "ConfigurableNamingEngine",
            check: Check::Mixed(configurable_naming_engine::naming_engine),
        },
        Rule {
            id: "SV-STY-001",
            name: "ClassMemberPrefix",
            check: Check::Model(class_member_prefix::class_member_prefix),
        },
        Rule {
            id: "SV-STY-002",
            name: "MacroTrailingSemicolon",
            check: Check::Syntax(macro_trailing_semicolon::macro_trailing_semicolon),
        },
        Rule {
            id: "SV-STY-003",
            name: "TodoCommentTracker",
            check: Check::Syntax(todo_comment_tracker::todo_comments),
        },
        Rule {
            id: "SV-STY-004",
            name: "TypedefNameSuffix",
            check: Check::Syntax(typedef_name_suffix::typedef_name_suffix),
        },
        Rule {
            id: "SV-STY-005",
            name: "MandatoryMatchingEndLabel",
            check: Check::Syntax(mandatory_matching_end_label::missing_end_label),
        },
        Rule {
            id: "SV-STY-006",
            name: "MismatchedEndLabel",
            check: Check::Syntax(mismatched_end_label::mismatched_end_label),
        },
        Rule { id: "SV-STY-007", name: "MethodBannerStyle", check: Check::Syntax(method_banner_style::method_banner) },
        Rule {
            id: "SV-STY-008",
            name: "SectionHeaderBanner",
            check: Check::Syntax(section_header_banner::section_banner),
        },
        Rule {
            id: "SV-STY-009",
            name: "ExternMethodThreshold",
            check: Check::Syntax(extern_method_threshold::extern_method_threshold),
        },
        Rule {
            id: "SV-STY-010",
            name: "ExplicitLiteralWidth",
            check: Check::Syntax(explicit_literal_width::explicit_literal_width),
        },
        Rule {
            id: "SV-STY-011",
            name: "OneStatementPerLine",
            check: Check::Syntax(one_statement_per_line::one_statement_per_line),
        },
        Rule {
            id: "SV-STY-012",
            name: "SpaceKeywordParen",
            check: Check::Syntax(space_keyword_paren::space_keyword_paren),
        },
        Rule { id: "SV-STY-013", name: "IncludeGuard", check: Check::Syntax(include_guard::include_guard) },
        Rule {
            id: "SV-STY-015",
            name: "TrailingWhitespace",
            check: Check::Syntax(trailing_whitespace::trailing_whitespace),
        },
        Rule { id: "SV-STY-016", name: "MaxLineLength", check: Check::Syntax(max_line_length::max_line_length) },
        Rule {
            id: "SV-STY-100",
            name: "ConfigurableStyleEngine",
            check: Check::Syntax(configurable_style_engine::style_engine),
        },
        Rule {
            id: "UVM-STY-001",
            name: "UvmCreateStringMismatch",
            check: Check::Syntax(uvm_create_string_mismatch::create_string_mismatch),
        },
        Rule {
            id: "UVM-STY-002",
            name: "ConstraintNameSuffix",
            check: Check::Model(constraint_name_suffix::constraint_name_suffix),
        },
        Rule {
            id: "UVM-TLM-003",
            name: "UvmTlmPortSuffix",
            check: Check::Syntax(uvm_tlm_port_suffix::tlm_port_suffix),
        },
    ]
}
