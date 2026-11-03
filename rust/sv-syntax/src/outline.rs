//! The declarations a file holds, in source order, in the same form as
//! the Slang reference tool (tools/cst_outline.cpp) prints them.

use crate::{NodeId, SyntaxKind, Tree};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutlineEntry {
    /// module, interface, program, package, class, function, task,
    /// typedef, constraint, covergroup.
    pub kind: &'static str,
    pub name: String,
    pub node: NodeId,
}

pub fn outline(tree: &Tree) -> Vec<OutlineEntry> {
    let mut out = Vec::new();
    for n in tree.preorder() {
        let kind = match tree.kind(n) {
            SyntaxKind::ModuleDecl => "module",
            SyntaxKind::InterfaceDecl => "interface",
            SyntaxKind::ProgramDecl => "program",
            SyntaxKind::PackageDecl => "package",
            SyntaxKind::ClassDecl => "class",
            SyntaxKind::TypedefDecl => "typedef",
            SyntaxKind::ConstraintDecl | SyntaxKind::ConstraintPrototype => "constraint",
            SyntaxKind::CovergroupDecl => "covergroup",
            SyntaxKind::TaskDecl => "task",
            SyntaxKind::FunctionDecl => "function",
            SyntaxKind::MethodPrototype => {
                let is_task = tree.token_range(n).iter().any(|&t| &*tree.tokens[t as usize].text == "task");
                if is_task {
                    "task"
                } else {
                    "function"
                }
            }
            _ => continue,
        };
        let name = tree.child(n, SyntaxKind::Name).map(|c| tree.bare_text(c)).unwrap_or_default();
        out.push(OutlineEntry { kind, name, node: n });
    }
    out
}
