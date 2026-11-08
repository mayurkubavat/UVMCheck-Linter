//! SV-DED-001 (UnusedVariable): Local variables declared in tasks or functions
//! should be referenced.

use std::collections::HashSet;
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::slang_ast::kids;
use crate::support::handles::{body_refs_if_good};

/// The classes slang's model lists: non-generic classes of `$unit`,
/// packages and other such classes (not of modules, not generic).
fn model_classes(c: &Cst) -> Vec<NodeId> {
    let generic = |k: NodeId| c.tree.child(k, Header).and_then(|h| c.tree.child(h, ParamList)).is_some();
    c.nodes(ClassDecl)
        .into_iter()
        .filter(|&k| !generic(k))
        .filter(|&k| {
            crate::support::slang_ast::ancestors(c, k)
                .all(|a| matches!(c.kind(a), SourceFile | PackageDecl) || (c.kind(a) == ClassDecl && !generic(a)))
        })
        .collect()
}

pub(crate) fn unused_variable(c: &Cst, out: &mut Sink) {
    for cls in model_classes(c) {
        for m in kids(c, cls).into_iter().filter(|&m| matches!(c.kind(m), FunctionDecl | TaskDecl)) {
            let Some(body) = c.tree.child(m, Body) else { continue };
            let mut locals = Vec::new();
            for d in kids(c, body).into_iter().filter(|&d| c.kind(d) == DataDecl) {
                let first = c.first_tok(d).map(|t| c.text_of(t)).unwrap_or("");
                if matches!(first, "input" | "output" | "inout" | "ref" | "typedef" | "localparam" | "parameter") {
                    continue;
                }
                for decl in c.tree.children(d).filter(|&x| c.kind(x) == Declarator) {
                    if let Some(nm) = c.tree.child(decl, Name) {
                        locals.push((decl, nm, c.tree.bare_text(nm)));
                    }
                }
            }
            if locals.is_empty() {
                continue;
            }
            let used = body_refs(c, body);
            let mname = last_name_of(c, m);
            for (decl, nm, name) in locals {
                if used.contains(&decl) {
                    continue;
                }
                let Some(loc) = c.first_tok(nm).map(|t| c.loc(t)) else { continue };
                out.add(
                    "SV-DED-001",
                    "UnusedVariable",
                    Severity::Info,
                    format!("Local variable '{name}' in method '{mname}' is declared but never referenced."),
                    loc,
                );
            }
        }
    }
}

/// The declarations slang's AST visit of a routine body reads (its
/// NamedValue expressions): not in declaration initializers (also a for
/// loop's); foreach loop variables shadow; none when a statement does not
/// bind (the whole body is then bad; an error in a declaration's
/// initializer does not count).
fn body_refs(c: &Cst, body: NodeId) -> HashSet<NodeId> {
    body_refs_if_good(c, body).unwrap_or_default()
}

fn last_name_of(c: &Cst, m: NodeId) -> String {
    crate::support::slang_ast::last_name(c, m)
}
