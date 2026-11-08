//! SV-BUG-005 (IntegerDivisionToReal): Flags integer divisions whose result is
//! used as a real value, which truncates before the conversion (Sonar S2184,
//! SpotBugs ICAST_IDIV_CAST_TO_DOUBLE).

use sv_syntax::NodeId;
use crate::support::class_items::{kids, op, user_code};
use crate::support::type_text::expr_text;
use crate::support::sema_facts::ast_ok;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;
use crate::support::real_types::{Ty, Types};

/// The division sits where slang converts it to real implicitly.
fn converted_to_real(types: &Types, div: NodeId) -> bool {
    let c = types.c;
    let mut x = div;
    while let Some(p) = c.parent(x).filter(|&p| c.kind(p) == ParenExpr) {
        x = p;
    }
    let Some(p) = c.parent(x) else { return false };
    match c.kind(p) {
        BinaryExpr => {
            let k = kids(c, p);
            let other = if k.first() == Some(&x) { k.get(1) } else { k.first() };
            match op(c, p) {
                "=" | "+=" | "-=" | "*=" | "/=" => {
                    k.get(1) == Some(&x) && k.first().is_some_and(|&l| types.expr(l) == Some(Ty::Real))
                }
                "+" | "-" | "*" | "/" | "**" | "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                    other.is_some_and(|&o| types.expr(o) == Some(Ty::Real))
                }
                _ => false,
            }
        }
        Declarator => c
            .parent(p)
            .filter(|&d| c.kind(d) == DataDecl)
            .and_then(|d| c.tree.child(d, DataType))
            .is_some_and(|dt| types.of_type(dt) == Some(Ty::Real)),
        ReturnStmt => {
            let mut f = p;
            while let Some(q) = c.parent(f) {
                if c.kind(q) == FunctionDecl {
                    return c
                        .tree
                        .child(q, ReturnType)
                        .and_then(|r| c.tree.child(r, DataType))
                        .is_some_and(|dt| types.of_type(dt) == Some(Ty::Real));
                }
                f = q;
            }
            false
        }
        _ => false,
    }
}

pub(crate) fn integer_division_to_real(c: &Cst, s: &mut Sink) {
    let types = Types::new(c);
    for n in c.nodes(BinaryExpr) {
        if op(c, n) != "/" || !user_code(c, n) {
            continue;
        }
        let k = kids(c, n);
        let (Some(&l), Some(&r)) = (k.first(), k.get(1)) else { continue };
        if types.expr(l) != Some(Ty::Int) || types.expr(r) != Some(Ty::Int) {
            continue;
        }
        if !converted_to_real(&types, n) || !ast_ok(c, n) {
            continue;
        }
        let Some(at) = c.first_tok(n) else { continue };
        // The text is the outermost parenthesized form; swe::exprText of the
        // operands is empty (they have no syntax of their own there).
        let mut outer = n;
        while let Some(p) = c.parent(outer).filter(|&p| c.kind(p) == ParenExpr) {
            outer = p;
        }
        s.add(
            "SV-BUG-005",
            "IntegerDivisionToReal",
            Severity::Warning,
            format!(
                "'{}' divides two integers, so the fraction is dropped before the result becomes real. Convert an \
                 operand first, for example 'real'() / '.",
                expr_text(c, outer)
            ),
            c.loc(at),
        );
    }
}
