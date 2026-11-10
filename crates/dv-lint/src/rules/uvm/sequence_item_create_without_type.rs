//! UVM-SEQ-010 (SequenceItemCreateWithoutType): Flags sequence items created
//! via direct new() inside sequence body().

use crate::support::uvm_classes::{sequence_bodies, Ix};
use crate::support::uvm_syntax::{assignment, body_items, name_node, stmt_expr, subtree};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use svlang::syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

/// slang's `NewClassExpression`: `new`, `new(...)`, `C::new(...)`.
fn is_new_class(c: &Cst, e: NodeId) -> bool {
    match c.kind(e) {
        NewExpr => {
            c.tok(e, "new").is_some() && c.tok(e, "[").is_none() && c.items(e).into_iter().all(|x| c.kind(x) == ArgList)
        }
        CallExpr => c.tree.children(e).next().is_some_and(|l| c.kind(l) == ScopedExpr && c.last_text(l) == "new"),
        _ => false,
    }
}

fn item_like(name: &str) -> bool {
    name == "req" || name == "rsp" || name.contains("item") || name.contains("tr") || name.contains("pkt")
}

pub(crate) fn item_create_without_type(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for c in csts {
        for (cls, f) in sequence_bodies(&ix, c) {
            for item in body_items(c, f) {
                for n in subtree(c, item) {
                    if let Some((_, lhs, rhs)) = stmt_expr(c, n).and_then(|e| assignment(c, e)) {
                        let l = c.trimmed(lhs);
                        if is_new_class(c, rhs) && item_like(&l) {
                            if let Some(t) = c.first_tok(rhs) {
                                s.add(
                                    "UVM-SEQ-010",
                                    "SequenceItemCreateWithoutType",
                                    Severity::Warning,
                                    format!(
                                        "Sequence item '{l}' in sequence '{cls}' body() is instantiated via direct 'new()'. \
                                         In UVM methodology (IEEE 1800.2 § 5.3), sequence items must be created through the factory \
                                         (e.g. '{l} = my_item::type_id::create(\"{l}\")' or 'create_item(...)') \
                                         to enable factory type overrides for error injection and stimulus extension."
                                    ),
                                    c.loc(t),
                                );
                            }
                        }
                    }
                    if c.kind(n) == DataDecl {
                        for dcl in c.tree.children(n).filter(|&x| c.kind(x) == Declarator) {
                            let Some(eq) = c.tok(dcl, "=") else { continue };
                            let Some(init) = c.node_after(dcl, eq) else { continue };
                            let var = name_node(c, dcl).map(|x| c.tree.bare_text(x)).unwrap_or_default();
                            if is_new_class(c, init) && item_like(&var) {
                                let Some(t) = c.first_tok(init) else { continue };
                                s.add(
                                    "UVM-SEQ-010",
                                    "SequenceItemCreateWithoutType",
                                    Severity::Warning,
                                    format!(
                                        "Sequence item '{var}' in sequence '{cls}' body() is initialized via direct 'new()'. \
                                         In UVM methodology (IEEE 1800.2 § 5.3), sequence items must be created through the factory \
                                         (e.g. 'type_id::create()') to enable factory overrides."
                                    ),
                                    c.loc(t),
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
