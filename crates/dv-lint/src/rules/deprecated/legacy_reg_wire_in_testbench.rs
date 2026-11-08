//! SV-BAN-001 (LegacyRegWireInTestbench): Discourages legacy 'reg' and 'wire'
//! keywords in SystemVerilog testbenches in favor of 'logic'.

use crate::support::names::DIRECTIONS;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::Child;
use sv_syntax::SyntaxKind::*;

pub(crate) fn legacy_reg_wire(c: &Cst, s: &mut Sink) {
    for n in c.nodes(DataDecl) {
        // A direction makes it slang's PortDeclaration, and a struct or
        // union member is a StructUnionMember.
        if c.toks(n).any(|t| DIRECTIONS.contains(&c.text_of(t))) || c.parent(n).is_some_and(|p| c.kind(p) == DataType) {
            continue;
        }
        if let Some(kw) = c.keyword(n) {
            // A net keyword first: slang's NetDeclaration.
            if c.text_of(kw) == "wire" && c.kids(n).first() == Some(&Child::Token(kw)) {
                s.add(
                    "SV-BAN-001",
                    "LegacyRegWireInTestbench",
                    Severity::Info,
                    "Keyword 'wire' is discouraged in SystemVerilog verification code. \
                     In testbenches and interfaces, use 'logic' or 'virtual interface' instead."
                        .into(),
                    c.loc(kw),
                );
                continue;
            }
        }
        let Some(ty) = c.tree.child(n, DataType) else { continue };
        if c.kids(n).first() != Some(&Child::Node(ty)) {
            continue;
        }
        let Some(first) = c.keyword(ty) else { continue };
        if c.text_of(first) == "reg" {
            s.add(
                "SV-BAN-001",
                "LegacyRegWireInTestbench",
                Severity::Info,
                "Legacy Verilog-95 keyword 'reg' is discouraged in modern SystemVerilog testbenches. \
                 Use 'logic' for unified 4-state storage across procedural and continuous assignments."
                    .into(),
                c.loc(first),
            );
        }
    }
}
