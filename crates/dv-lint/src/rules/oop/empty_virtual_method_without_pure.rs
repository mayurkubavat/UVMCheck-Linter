//! SV-OOP-009 (EmptyVirtualMethodWithoutPure): Flags virtual methods defined
//! with empty bodies instead of being declared 'pure virtual' in abstract base
//! classes.

use crate::support::class_items::{class_decls, decl_name, own};
use crate::support::data_items::{methods, routine_name};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind::*;

pub(crate) fn empty_virtual(c: &Cst, s: &mut Sink) {
    for cls in class_decls(c) {
        let cname = decl_name(c, cls).map(|x| x.0).unwrap_or_default();
        for f in methods(c, cls) {
            let o = own(c, f);
            if !o.contains(&"virtual") || o.contains(&"pure") {
                continue;
            }
            if !o.iter().any(|t| matches!(*t, "endfunction" | "endtask")) {
                continue;
            }
            let Some(body) = c.tree.child(f, Body) else { continue };
            if c.tree.children(body).next().is_some() {
                continue;
            }
            let name = routine_name(c, f);
            if name.is_empty() || name == "new" {
                continue;
            }
            let Some(kw) = c.toks(f).find(|&t| matches!(c.text_of(t), "function" | "task")) else { continue };
            s.add(
                "SV-OOP-009",
                "EmptyVirtualMethodWithoutPure",
                Severity::Info,
                format!(
                    "Virtual method '{name}' in class '{cname}' is defined with an empty body without the 'pure' keyword. Empty stub methods allow derived classes to silently omit required overrides. Declare as 'pure virtual' in interface base classes or provide meaningful default logic."
                ),
                c.loc(kw),
            );
        }
    }
}
