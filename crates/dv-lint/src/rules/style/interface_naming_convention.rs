//! SV-NAM-003 (InterfaceNamingConvention): Enforces that interface identifiers
//! end with '_if' (e.g., 'axi_if') to standardize interface definitions.

use crate::support::names::name_tok;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn interface_naming(c: &Cst, s: &mut Sink) {
    for n in c.nodes(InterfaceDecl) {
        let Some(t) = name_tok(c, n) else { continue };
        let name = c.text_of(t);
        if !name.is_empty() && !name.ends_with("_if") {
            s.add(
                "SV-NAM-003",
                "InterfaceNamingConvention",
                Severity::Warning,
                format!(
                    "Interface '{name}' does not end with required suffix '_if'. \
                     SystemVerilog methodology standards require interface identifiers to end with '_if' (e.g. '{name}_if') to distinguish interfaces from modules and testbench classes."
                ),
                c.loc(t),
            );
        }
    }
}
