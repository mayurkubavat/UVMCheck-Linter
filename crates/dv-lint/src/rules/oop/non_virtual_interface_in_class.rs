//! SV-OOP-008 (NonVirtualInterfaceInClass): Disallows declaring physical
//! interface types inside classes without the 'virtual' keyword (IEEE 1800 §
//! 25.9).

use crate::support::class_items::{class_decls, decl_name, declarators, op, qualifiers};
use crate::support::data_items::data_items;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind::*;

pub(crate) fn non_virtual_interface(c: &Cst, s: &mut Sink) {
    for cls in class_decls(c) {
        let cname = decl_name(c, cls).map(|x| x.0).unwrap_or_default();
        for d in data_items(c, cls) {
            let dt = c.tree.child(d, DataType).unwrap();
            let virt = qualifiers(c, d).contains(&"virtual") || op(c, dt) == "virtual";
            if virt {
                continue;
            }
            let ty = c.last_text(dt);
            // An interface by its declaration; by its name (`_if`, `_vif`) only
            // when nothing in the compile declares the type.
            let interface =
                crate::sem::is_interface(c, ty).unwrap_or_else(|| ty.ends_with("_if") || ty.ends_with("_vif"));
            if ty.starts_with("virtual") || !interface {
                continue;
            }
            // Unsafe: the member becomes a handle that must be set (config_db) before use.
            let fix = c.first_tok(dt).filter(|&t| !c.tree.tokens[t as usize].from_macro).map(|t| {
                let tok = &c.tree.tokens[t as usize];
                let at = tok.origin.offset as usize;
                let mut f = c.file_fix(
                    tok.origin.file,
                    at,
                    at,
                    "virtual ",
                    "Insert 'virtual' keyword to declare a virtual interface handle",
                );
                f.safety = crate::fix::FixSafety::Unsafe;
                f
            });
            for (_, var, at) in declarators(c, d) {
                s.add(
                    "SV-OOP-008",
                    "NonVirtualInterfaceInClass",
                    Severity::Error,
                    format!(
                        "Interface property '{var}' of type '{ty}' in class '{cname}' is declared without the 'virtual' keyword. In SystemVerilog (IEEE 1800 § 25.9), classes cannot instantiate physical interfaces; only virtual interface handles ('virtual {ty} {var};') can be stored as class members."
                    ),
                    c.loc(at),
                )
                .fixes
                .extend(fix.clone());
            }
        }
    }
}
