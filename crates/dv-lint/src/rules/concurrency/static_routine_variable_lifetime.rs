//! SV-THR-002 (StaticRoutineVariableLifetime): Non-class tasks and functions
//! default to static lifetime in SystemVerilog.

use sv_syntax::SyntaxKind;
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::procedural::{add_n, routine_name};

pub(crate) fn static_routine_variable(c: &Cst, s: &mut Sink) {
    for k in [FunctionDecl, TaskDecl] {
        for r in c.nodes(k) {
            if enclosing_class(c, r).is_some() {
                continue;
            }
            if has_kw(c, r, "automatic") {
                continue;
            }
            // The routine's default lifetime is its unit's.
            let unit = ancestors(c, r).find(|&a| matches!(c.kind(a), ModuleDecl | InterfaceDecl | ProgramDecl | PackageDecl | CheckerDecl));
            if unit.is_some_and(|u| has_kw(c, u, "automatic")) {
                continue;
            }
            let Some(body) = c.tree.child(r, Body) else { continue };
            let n = times(c, r);
            let rname = routine_name(c, r);
            for d in kids(c, body) {
                if c.kind(d) != DataDecl {
                    continue;
                }
                let toks = tok_texts(c, d);
                if toks.iter().any(|t| matches!(*t, "automatic" | "const" | "typedef" | "input" | "output" | "inout" | "ref")) {
                    continue;
                }
                if c.tree.child(d, DataType).is_some_and(|t| tok_texts(c, t).first() == Some(&"const")) {
                    continue;
                }
                for decl in c.tree.children(d).filter(|&x| c.kind(x) == Declarator) {
                    let Some(nm) = c.tree.child(decl, Name) else { continue };
                    let Some(t) = c.first_tok(nm) else { continue };
                    let vname = c.text_of(t);
                    add_n(
                        s,
                        n,
                        "SV-THR-002",
                        "StaticRoutineVariableLifetime",
                        Severity::Error,
                        format!(
                            "Local variable '{vname}' in routine '{rname}' has implicit static lifetime. In non-class scopes, \
                             tasks and functions default to static lifetime, causing local variables to be shared \
                             across concurrent invocations and inducing race conditions. Declare routine as 'automatic'."
                        ),
                        c.loc(t),
                    );
                }
            }
        }
    }
}
