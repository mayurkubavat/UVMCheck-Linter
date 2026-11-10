//! Declared types of class properties.

use svlang::syntax::{NodeId, SyntaxKind};

use crate::support::slang_ast::decl_name;

use crate::support::class_index::Classes;

use crate::support::class_items::{declarators, kids};

use crate::syntax::Cst;

use SyntaxKind::*;

/// The declared type of the variable a name stands for at `at`: a formal
/// or local of the routine around it, a property of the class around it
/// (or a class it extends), else a declaration of an enclosing scope.
pub(crate) fn declared_type(c: &Cst, classes: &Classes, at: NodeId, name: &str) -> Option<NodeId> {
    let mut x = at;
    while let Some(p) = c.parent(x) {
        match c.kind(p) {
            FunctionDecl | TaskDecl => {
                for d in crate::support::statements::subtree(c, p) {
                    if matches!(c.kind(d), DataDecl | Port) && declarators(c, d).iter().any(|(_, v, _)| v == name) {
                        return c.tree.child(d, DataType);
                    }
                }
                // `function C::f`: the members of C.
                let full = decl_name(c, p);
                if let Some((cls, _)) = full.rsplit_once("::") {
                    if let Some(k) = classes.find(cls) {
                        if let Some(t) = class_property(classes, k, name) {
                            return t;
                        }
                    }
                }
            }
            ClassDecl => {
                let k = classes.list.iter().position(|k| k.node == p)?;
                if let Some(t) = class_property(classes, k, name) {
                    return t;
                }
            }
            SeqBlock | ParBlock | ModuleDecl | InterfaceDecl | ProgramDecl | PackageDecl | SourceFile => {
                for d in kids(c, p) {
                    if c.kind(d) == DataDecl && declarators(c, d).iter().any(|(_, v, _)| v == name) {
                        return c.tree.child(d, DataType);
                    }
                }
            }
            _ => {}
        }
        x = p;
    }
    None
}

pub(crate) fn class_property(classes: &Classes, i: usize, name: &str) -> Option<Option<NodeId>> {
    let mut cur = Some(i);
    for _ in 0..64 {
        let k = cur?;
        if let Some(p) = classes.list[k].props.iter().find(|p| p.name == name) {
            return Some(p.ty);
        }
        cur = classes.base(k);
    }
    None
}
