//! uvmcheck's default style configuration, for the configurable style engine
//! (SV-STY-100 and its sub-rules SV-STY-101..106).

use svlang::syntax::NodeId;

use svlang::syntax::SyntaxKind::*;

use crate::syntax::Cst;
use crate::{Severity, Sub};

/// The ids SV-STY-100 reports under. SV-STY-105 and SV-STY-106 are off
/// unless the project config sets style.file_matches_class_name /
/// style.require_file_header.
pub(crate) const STYLE_SUBS: &[Sub] = &[
    Sub { id: "SV-STY-101", name: "NoTabs", finding_name: "TabCharactersProhibited", severity: Severity::Warning, about: "Literal tab characters are not allowed (style.allow_tabs)." },
    Sub { id: "SV-STY-102", name: "NoTrailingWhitespace", finding_name: "TrailingWhitespace", severity: Severity::Warning, about: "Lines must not end in whitespace (style.allow_trailing_whitespace)." },
    Sub { id: "SV-STY-103", name: "MaxLineLength", finding_name: "MaxLineLengthExceeded", severity: Severity::Warning, about: "Lines must not exceed style.max_line_length characters." },
    Sub { id: "SV-STY-104", name: "MatchingEndLabel", finding_name: "MandatoryMatchingEndLabel", severity: Severity::Warning, about: "Blocks, routines and classes carry an end label that matches their name (style.mandatory_matching_end_label)." },
    Sub { id: "SV-STY-105", name: "FileMatchesClassName", finding_name: "FileMatchesClassName", severity: Severity::Warning, about: "A file's base name matches the top-level class it defines." },
    Sub { id: "SV-STY-106", name: "FileHeader", finding_name: "MissingFileHeader", severity: Severity::Warning, about: "Files start with a header matching style.file_header_pattern (style.require_file_header)." },
];

/// The ids SV-NAM-100 reports under (SV-NAM-104 is off by default).
pub(crate) const NAMING_SUBS: &[Sub] = &[
    Sub {
        id: "SV-NAM-101",
        name: "PackageNaming",
        finding_name: "PackageNaming",
        severity: Severity::Warning,
        about: "Packages should end with '_pkg' for clean namespace identification.",
    },
    Sub {
        id: "SV-NAM-102",
        name: "InterfaceNaming",
        finding_name: "InterfaceNaming",
        severity: Severity::Warning,
        about: "Interfaces should end with '_if' suffix.",
    },
    Sub {
        id: "SV-NAM-103",
        name: "VirtualInterfaceNaming",
        finding_name: "VirtualInterfaceNaming",
        severity: Severity::Warning,
        about: "Virtual interface handles and typedefs should end with '_vif'.",
    },
    Sub {
        id: "SV-NAM-104",
        name: "ClassNaming",
        finding_name: "ClassNaming",
        severity: Severity::Warning,
        about: "General classes should follow PascalCase naming conventions.",
    },
    Sub {
        id: "SV-NAM-105",
        name: "CovergroupNaming",
        finding_name: "CovergroupNaming",
        severity: Severity::Warning,
        about: "Covergroup declarations must end with '_cg' suffix.",
    },
    Sub {
        id: "SV-NAM-106",
        name: "CoverpointNaming",
        finding_name: "CoverpointNaming",
        severity: Severity::Warning,
        about: "Coverpoints should start with 'cp_' prefix.",
    },
    Sub {
        id: "SV-NAM-107",
        name: "CrossNaming",
        finding_name: "CrossNaming",
        severity: Severity::Warning,
        about: "Cross coverages should start with 'cross_' or 'cx_' prefix.",
    },
    Sub {
        id: "SV-NAM-108",
        name: "AssertionNaming",
        finding_name: "AssertionNaming",
        severity: Severity::Warning,
        about: "Concurrent assertions should have labels ending with '_a' or '_assert'.",
    },
    Sub {
        id: "SV-NAM-109",
        name: "ParameterNaming",
        finding_name: "ParameterNaming",
        severity: Severity::Warning,
        about: "Parameters and constants should follow ALL_CAPS conventions.",
    },
    Sub {
        id: "SV-NAM-110",
        name: "TypedefEnumNaming",
        finding_name: "TypedefEnumNaming",
        severity: Severity::Warning,
        about: "Enum typedefs should end with '_e' suffix.",
    },
    Sub {
        id: "SV-NAM-111",
        name: "TypedefNaming",
        finding_name: "TypedefNaming",
        severity: Severity::Warning,
        about: "User-defined typedefs should end with '_t' suffix.",
    },
    Sub {
        id: "SV-NAM-112",
        name: "ConstraintNaming",
        finding_name: "ConstraintNaming",
        severity: Severity::Warning,
        about: "Class constraint blocks should end with '_c' suffix.",
    },
    Sub {
        id: "UVM-NAM-101",
        name: "UvmDriverNaming",
        finding_name: "UvmDriverNaming",
        severity: Severity::Warning,
        about: "UVM driver components must end with '_driver' or '_drv'.",
    },
    Sub {
        id: "UVM-NAM-102",
        name: "UvmMonitorNaming",
        finding_name: "UvmMonitorNaming",
        severity: Severity::Warning,
        about: "UVM monitor components must end with '_monitor' or '_mon'.",
    },
    Sub {
        id: "UVM-NAM-103",
        name: "UvmSequencerNaming",
        finding_name: "UvmSequencerNaming",
        severity: Severity::Warning,
        about: "UVM sequencers must end with '_sequencer' or '_sqr'.",
    },
    Sub {
        id: "UVM-NAM-104",
        name: "UvmSequenceNaming",
        finding_name: "UvmSequenceNaming",
        severity: Severity::Warning,
        about: "UVM sequence classes must end with '_seq' or '_sequence'.",
    },
    Sub {
        id: "UVM-NAM-105",
        name: "UvmSequenceItemNaming",
        finding_name: "UvmSequenceItemNaming",
        severity: Severity::Warning,
        about: "UVM transaction/sequence items must end with '_item' or '_seq_item'.",
    },
    Sub {
        id: "UVM-NAM-106",
        name: "UvmAgentNaming",
        finding_name: "UvmAgentNaming",
        severity: Severity::Warning,
        about: "UVM agents must end with '_agent' suffix.",
    },
    Sub {
        id: "UVM-NAM-107",
        name: "UvmEnvNaming",
        finding_name: "UvmEnvNaming",
        severity: Severity::Warning,
        about: "UVM environments must end with '_env' suffix.",
    },
    Sub {
        id: "UVM-NAM-108",
        name: "UvmTestNaming",
        finding_name: "UvmTestNaming",
        severity: Severity::Warning,
        about: "UVM test classes must end with '_test' suffix.",
    },
    Sub {
        id: "UVM-NAM-109",
        name: "UvmScoreboardNaming",
        finding_name: "UvmScoreboardNaming",
        severity: Severity::Warning,
        about: "UVM scoreboards must end with '_scoreboard' or '_scb'.",
    },
];

/// The id SV-CMP-001 reports compiler warnings under.
pub(crate) const COMPILER_SUBS: &[Sub] = &[Sub {
    id: "SV-CMP-002",
    name: "CompilerWarning",
    finding_name: "CompilerWarning",
    severity: Severity::Warning,
    about: "Reports the SystemVerilog compiler's warnings as findings (controlled with -W options).",
}];

/// How often slang's AST visit reaches `n`: per instance (with generate
/// copies) in a module, else once per specialization of each generic class
/// around it.
pub(crate) fn ast_visits(c: &Cst, n: NodeId) -> usize {
    let anc: Vec<NodeId> = crate::support::slang_ast::ancestors(c, n).collect();
    if anc.iter().any(|&a| matches!(c.kind(a), ModuleDecl | InterfaceDecl | ProgramDecl | CheckerDecl)) {
        // The elaborated copies: generate loops unrolled per instance.
        if let Some(m) = crate::support::slang_ast::elab_copies(c, n) {
            return m;
        }
        return crate::support::slang_ast::times(c, n);
    }
    // The class around it, or the one an out-of-block method body names.
    let Some(k) = crate::support::slang_ast::enclosing_class(c, n) else { return 1 };
    std::iter::once(k)
        .chain(crate::support::slang_ast::ancestors(c, k))
        .filter(|&a| c.kind(a) == ClassDecl)
        .map(|a| c.spec_count(a).unwrap_or(1))
        .product()
}
