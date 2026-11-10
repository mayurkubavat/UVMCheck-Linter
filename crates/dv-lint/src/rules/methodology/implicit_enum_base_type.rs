//! SV-CNS-006 (ImplicitEnumBaseType): Flags enums in packages, classes and
//! interfaces declared without a base type.

use crate::support::defines_packages::{container_of, user_types, Container, Files};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use svlang::syntax::SyntaxKind::*;

pub(crate) fn implicit_enum_base_type(_: &Design, csts: &[Cst], out: &mut Sink) {
    for c in csts {
        let fs = Files::new(c);
        for (n, kw) in user_types(c, &fs, "enum") {
            if c.tree.child(n, DataType).is_some() || container_of(c, n) == Container::Module {
                continue;
            }
            let first = c
                .tree
                .child(n, Declarator)
                .and_then(|d| c.tree.child(d, Name))
                .map(|x| format!(" {{{}, ...}}", c.tree.bare_text(x)))
                .unwrap_or_default();
            out.add(
                "SV-CNS-006",
                "ImplicitEnumBaseType",
                Severity::Info,
                format!(
                    "Enum{first} has no base type, so it is a 32-bit int. State the width it models, for example enum bit [1:0] {{...}}."
                ),
                c.loc(kw),
            );
        }
    }
}
