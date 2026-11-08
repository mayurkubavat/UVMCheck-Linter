//! SV-DSN-012 (PrimitiveObsession): Flags a rand integral field of a sequence
//! item or config that a constraint limits to a list of 3 to 16 single constant
//! values ('x inside {0, 1, 2}').

use std::collections::HashSet;
use sv_syntax::{NodeId, SyntaxKind};
use crate::support::statements::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::class_index::{Classes, Role, referenced_name};

pub(crate) fn primitive_obsession(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    for i in classes.user() {
        let role = classes.role(i);
        if !matches!(role, Role::Item | Role::Config | Role::Object) {
            continue;
        }
        let cls = &classes.list[i];
        let mut reported: HashSet<String> = HashSet::new();
        for cd in c.tree.children(cls.node).filter(|&x| c.kind(x) == ConstraintDecl) {
            let Some(block) = c.tree.child(cd, ConstraintBlock) else {
                continue;
            };
            for item in c.tree.children(block).filter(|&x| c.kind(x) == ConstraintItem) {
                let Some(e) = c.tree.children(item).next() else {
                    continue;
                };
                if c.tree.children(item).count() != 1 {
                    continue;
                }
                let e = strip_parens(c, e);
                if c.kind(e) != InsideExpr {
                    continue;
                }
                let mut parts = c.tree.children(e);
                let (Some(l), Some(rl)) = (parts.next(), parts.next()) else {
                    continue;
                };
                let Some(field) = referenced_name(c, l) else { continue };
                let Some(p) = cls.props.iter().find(|p| p.name == field) else {
                    continue;
                };
                if !p.rand {
                    continue;
                }
                let Some(ty) = p.ty else { continue };
                if !multi_bit_integral(c, ty) {
                    continue;
                }
                let items: Vec<NodeId> = c.tree.children(rl).collect();
                if items.len() < 3 || items.len() > 16 {
                    continue;
                }
                if items.iter().any(|&x| c.kind(x) == OpenRange || !constant_expr(c, x)) {
                    continue;
                }
                if !reported.insert(field.to_string()) {
                    continue;
                }
                s.add(
                    "SV-DSN-012",
                    "PrimitiveObsession",
                    Severity::Info,
                    format!(
                        "'{field}' only ever takes {} listed values, so it is an enumeration written as a bit vector. Declare a typedef \
                         enum: logs and waveforms then show names instead of numbers, and a wrong value no \
                         longer compiles.",
                        items.len()
                    ),
                    c.loc(p.tok),
                );
            }
        }
    }
}

/// An integral, non-enum type wider than one bit, as written.
fn multi_bit_integral(c: &Cst, ty: NodeId) -> bool {
    let Some(first) = c.first_tok(ty).map(|t| c.text_of(t)) else {
        return false;
    };
    match first {
        "int" | "integer" | "shortint" | "longint" | "byte" => true,
        "bit" | "logic" | "reg" => c
            .tree
            .children(ty)
            .any(|d| c.kind(d) == Dim && dim_wider_than_one(c, d)),
        _ => false,
    }
}

fn dim_wider_than_one(c: &Cst, d: NodeId) -> bool {
    let v: Vec<NodeId> = c.tree.children(d).collect();
    if v.len() != 2 {
        return true;
    }
    let (a, b) = (c.trimmed(v[0]), c.trimmed(v[1]));
    a != b
}

/// `swe::isConstantExpr` on the syntax: literals and operators of them.
fn constant_expr(c: &Cst, e: NodeId) -> bool {
    let e = strip_parens(c, e);
    match c.kind(e) {
        Literal => true,
        BinaryExpr => c.tree.children(e).all(|x| constant_expr(c, x)),
        PrefixExpr => c.tree.children(e).all(|x| constant_expr(c, x)),
        _ => false,
    }
}
