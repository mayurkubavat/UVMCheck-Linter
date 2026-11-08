//! uvmcheck's default style configuration, for the configurable style engine
//! (SV-STY-100 and its sub-rules SV-STY-101..106).

use sv_syntax::NodeId;

use sv_syntax::SyntaxKind::*;

use crate::syntax::Cst;

/// The ids SV-STY-100 reports under.
pub(crate) const STYLE_SUBS: &[(&str, &str)] = &[
    ("SV-STY-101", "TabCharactersProhibited"),
    ("SV-STY-102", "TrailingWhitespace"),
    ("SV-STY-103", "MaxLineLengthExceeded"),
    ("SV-STY-104", "MandatoryMatchingEndLabel"),
    // Off unless the project config sets style.file_matches_class_name /
    // style.require_file_header.
    ("SV-STY-105", "FileMatchesClassName"),
    ("SV-STY-106", "MissingFileHeader"),
];

/// The ids SV-NAM-100 reports under (the default naming rules; SV-NAM-104
/// is off by default and SV-NAM-103 has no check).
pub(crate) const NAMING_SUBS: &[(&str, &str)] = &[
    ("SV-NAM-101", "PackageNaming"),
    ("SV-NAM-102", "InterfaceNaming"),
    ("SV-NAM-105", "CovergroupNaming"),
    ("SV-NAM-106", "CoverpointNaming"),
    ("SV-NAM-107", "CrossNaming"),
    ("SV-NAM-109", "ParameterNaming"),
    ("SV-NAM-110", "TypedefEnumNaming"),
    ("SV-NAM-111", "TypedefNaming"),
    ("SV-NAM-112", "ConstraintNaming"),
    ("UVM-NAM-101", "UvmDriverNaming"),
    ("UVM-NAM-102", "UvmMonitorNaming"),
    ("UVM-NAM-103", "UvmSequencerNaming"),
    ("UVM-NAM-104", "UvmSequenceNaming"),
    ("UVM-NAM-105", "UvmSequenceItemNaming"),
    ("UVM-NAM-106", "UvmAgentNaming"),
    ("UVM-NAM-107", "UvmEnvNaming"),
    ("UVM-NAM-108", "UvmTestNaming"),
    ("UVM-NAM-109", "UvmScoreboardNaming"),
];

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
