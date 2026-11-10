//! UVM-CPL-004 (HierarchicalReferenceInClass): Flags class code that names a
//! module hierarchy (tb_top.dut.sig, $root.tb.x).

use crate::support::class_roles::{Ctx, Index};
use crate::support::free_routines::property_owner;
use crate::support::routine_bodies::bodies;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use std::collections::HashSet;
use svlang::syntax::SyntaxKind::*;

pub(crate) fn hierarchical_reference_in_class(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    let mut units = HashSet::new();
    for c in csts {
        for n in c.nodes_of(&[ModuleDecl, InterfaceDecl, ProgramDecl]) {
            if let Some(name) = c.tree.child(n, Name) {
                units.insert(c.tree.bare_text(name));
            }
        }
    }
    for (cls, m, c, body) in bodies(d, &ix) {
        let ctx = Ctx { d, cls, m: Some(m), td: Some(&ix.typedefs) };
        for n in c.subtree(body) {
            if c.kind(n) != MemberExpr || c.parent(n).is_some_and(|p| c.kind(p) == MemberExpr) {
                continue;
            }
            if c.parent(n).is_some_and(|p| c.kind(p) == CallExpr && c.tree.children(p).next() == Some(n)) {
                continue;
            }
            let mut root = n;
            while c.kind(root) == MemberExpr {
                match c.tree.children(root).next() {
                    Some(x) => root = x,
                    None => break,
                }
            }
            if c.kind(root) != NameExpr || c.tree.children(root).next().is_some() {
                continue;
            }
            let name = c.tree.bare_text(root);
            let hier = name == "$root"
                || (units.contains(&name)
                    && ctx.class_of_name(&name).is_none()
                    && !m.locals.iter().any(|l| l.name == name)
                    && !m.arguments.iter().any(|a| a.name == name)
                    && property_owner(d, cls, &name).is_none());
            if !hier {
                continue;
            }
            out.add(
                "UVM-CPL-004",
                "HierarchicalReferenceInClass",
                Severity::Warning,
                format!(
                    "{}::{} reads '{}' through the module hierarchy. The class then only compiles and works in one testbench top; pass signals in through a \
virtual interface from config_db instead.",
                    cls.name,
                    m.name,
                    c.expr_text(n)
                ),
                c.loc_of(n),
            );
        }
    }
}
