//! SV-OOP-013 (AbstractClassInstantiation): Disallows direct instantiation of
//! abstract (virtual) classes via 'new()'.

use crate::support::class_fields::{class_property, declared_type};
use crate::support::class_index::Classes;
use crate::support::class_items::{kids, op};
use crate::support::data_items::unparen;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

/// The type a `new` constructs: the declared type of what it is assigned
/// to (`x = new`, `T x = new`).
fn new_target(c: &Cst, classes: &Classes, n: NodeId) -> Option<NodeId> {
    let p = c.parent(n)?;
    match c.kind(p) {
        BinaryExpr if op(c, p) == "=" && kids(c, p).get(1) == Some(&n) => {
            let lhs = unparen(c, *kids(c, p).first()?);
            match c.kind(lhs) {
                NameExpr => {
                    let toks: Vec<u32> = c.toks(lhs).collect();
                    let [t] = toks[..] else { return None };
                    declared_type(c, classes, n, c.text_of(t))
                }
                MemberExpr => {
                    let base = c.tree.children(lhs).next()?;
                    if c.kind(base) != NameExpr || c.tree.bare_text(base) != "this" {
                        return None;
                    }
                    let (_, last) = c.tree.token_bounds(lhs)?;
                    let k = classes.enclosing(c, n)?;
                    class_property(classes, k, c.text_of(last))?
                }
                _ => None,
            }
        }
        Declarator => {
            let d = c.parent(p)?;
            (c.kind(d) == DataDecl).then(|| c.tree.child(d, DataType)).flatten()
        }
        _ => None,
    }
}

/// slang's NewVirtualClass error, once per location (diagnostics of every
/// specialization are merged by location).
pub(crate) fn abstract_new(c: &Cst, s: &mut Sink) {
    let news = c.nodes(NewExpr);
    if news.is_empty() {
        return;
    }
    let classes = Classes::new(c);
    let mut model: Option<sv_sema::SpecModel> = None;
    for n in news {
        // `new[n]`, `new obj` (a copy) and `super.new` call no constructor.
        if c.toks(n).any(|t| c.text_of(t) == "[") || c.tree.child(n, ArgList).is_none() && !kids(c, n).is_empty() {
            continue;
        }
        if c.parent(n).is_some_and(|p| c.kind(p) == MemberExpr) {
            continue;
        }
        let Some(dt) = new_target(c, &classes, n) else { continue };
        let m = model.get_or_insert_with(|| sv_sema::SpecModel::new(c.all));
        // The class may be declared in another tree of the compile.
        let any_virtual = m.class_per_named_spec(c.index(), dt).into_iter().flatten().any(|(ti, cls)| {
            let t = &c.all[ti];
            let own = |w: &str| {
                t.node(cls)
                    .children
                    .iter()
                    .any(|k| matches!(*k, sv_syntax::Child::Token(x) if &*t.tokens[x as usize].text == w))
            };
            own("virtual") && own("class")
        });
        if !any_virtual {
            continue;
        }
        let Some(at) = c.first_tok(n) else { continue };
        s.add(
            "SV-OOP-013",
            "AbstractClassInstantiation",
            Severity::Error,
            "Cannot instantiate abstract (virtual) class via 'new()'. In SystemVerilog (IEEE 1800 § 8.21), virtual \
             classes cannot have direct instances; instantiate a concrete derived class instead."
                .to_string(),
            c.loc(at),
        );
    }
}
