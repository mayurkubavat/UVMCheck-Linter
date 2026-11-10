//! UVM-PAT-002 (HandRolledSingleton): Flags a class that keeps a static
//! variable of its own type, the usual hand-rolled singleton.

use crate::support::class_roles::{class_methods, user_classes, Ctx, Index};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::{Class, Design};
use svlang::syntax::SyntaxKind::*;

fn singleton_finding(cls: &Class, var: &str, loc: dv_model::Loc, out: &mut Sink) {
    out.add(
        "UVM-PAT-002",
        "HandRolledSingleton",
        Severity::Warning,
        format!(
            "Class '{}' keeps a static instance of itself in '{var}'. Every env and test shares that one object, and a test \
cannot substitute another. Create it in the test or env and pass it through uvm_config_db or the resource database.",
            cls.name
        ),
        loc,
    );
}

pub(crate) fn hand_rolled_singleton(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for cls in user_classes(d) {
        for p in &cls.properties {
            if p.is_static && !p.is_const && p.type_class == Some(cls.id) {
                singleton_finding(cls, &p.name, p.loc, out);
            }
        }
        // Static locals declared at the top of a method body (not extern ones).
        for m in class_methods(cls).filter(|m| !m.is_extern) {
            let Some((c, body)) = ix.body(m) else { continue };
            let ctx = Ctx { d, cls, m: Some(m), td: Some(&ix.typedefs) };
            for dd in c.items(body).into_iter().filter(|&x| c.kind(x) == DataDecl) {
                if c.tok(dd, "static").is_none() || c.tok(dd, "const").is_some() {
                    continue;
                }
                let Some(ty) = c.tree.child(dd, DataType) else { continue };
                if ctx.class_of_type(&c.tree.bare_text(ty)).map(|k| k.id) != Some(cls.id) {
                    continue;
                }
                for dcl in c.tree.children(dd).filter(|&x| c.kind(x) == Declarator) {
                    if c.tree.child(dcl, Dim).is_some() {
                        continue;
                    }
                    let Some(name) = c.tree.child(dcl, Name) else { continue };
                    singleton_finding(cls, &c.tree.bare_text(name), c.loc_of(name), out);
                }
            }
        }
    }
}
