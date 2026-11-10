//! SV-NAM-002 (PackageNamingConvention): Enforces that package identifiers end
//! with '_pkg' (e.g., 'axi_pkg') to standardize namespaces.

use crate::support::names::name_tok;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind::*;

pub(crate) fn package_naming(c: &Cst, s: &mut Sink) {
    for n in c.nodes(PackageDecl) {
        let Some(t) = name_tok(c, n) else { continue };
        let name = c.text_of(t);
        if !name.is_empty() && !name.ends_with("_pkg") {
            s.add(
                "SV-NAM-002",
                "PackageNamingConvention",
                Severity::Warning,
                format!(
                    "Package '{name}' does not end with required suffix '_pkg'. \
                     SystemVerilog methodology standards require package identifiers to end with '_pkg' (e.g. '{name}_pkg') to clarify namespace scopes and prevent collision with modules."
                ),
                c.loc(t),
            );
        }
    }
}
