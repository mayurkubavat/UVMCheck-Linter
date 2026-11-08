//! SV-PRF-009 (NbaOnClassProperty): Disallow non-blocking assignments ('<=') on
//! class properties.

use std::collections::HashSet;
use sv_syntax::SyntaxKind;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use SyntaxKind::*;
use crate::support::ast::{Ast, DeclKind, assignment_op, in_class, kids, symbol_ref};

/// Slang skips a body it rejects, so only a non-blocking assignment the
/// model kept (its body compiles) is reported; the model holds no module code.
pub(crate) fn ast_nba_on_property(d: &Design, csts: &[Cst], s: &mut Sink) {
    let kept: HashSet<(u32, u32, u32)> = d
        .classes()
        .iter()
        .flat_map(|k| k.methods.iter())
        .flat_map(|m| m.assignments.iter())
        .filter(|x| x.is_non_blocking)
        .map(|x| (x.loc.file, x.loc.line, x.loc.column))
        .collect();
    for c in csts {
        nba_on_property(c, &Ast::new(d, c), &kept, s);
    }
}

fn nba_on_property(c: &Cst, a: &Ast, kept: &HashSet<(u32, u32, u32)>, s: &mut Sink) {
    for n in c.nodes(BinaryExpr) {
        if assignment_op(c, n) != "<=" || !c.parent(n).is_some_and(|p| c.kind(p) == ExprStmt) {
            continue;
        }
        let Some(&lhs) = kids(c, n).first() else { continue };
        let Some((d, name)) = symbol_ref(c, lhs) else { continue };
        if d.kind != DeclKind::Prop {
            continue;
        }
        let Some(at) = c.first_tok(lhs) else { continue };
        let l = c.loc(at);
        if in_class(c, n) && !kept.contains(&(l.file, l.line, l.column)) {
            continue;
        }
        s.add(
            "SV-PRF-009",
            "NbaOnClassProperty",
            Severity::Warning,
            format!(
                "Non-blocking assignment ('<=') targeting class property '{name}'. \
                 Unlike static module/interface registers, class instances reside on the dynamic heap. \
                 Performing non-blocking assignments on class members forces the simulation kernel to allocate \
                 dynamic event descriptors in the NBA event queue, causing severe scheduler queue thrashing. \
                 Replace with blocking assignment ('=')."
            ),
            c.loc(at),
        );
        a.again(s, c, n);
    }
}
