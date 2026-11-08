//! SV-DSN-011 (FeatureEnvy): Flags a method that uses one other object's fields
//! at least 6 times (option min_foreign) and more than twice as often as its
//! own class's fields.

use std::collections::BTreeMap;
use dv_model::{Class, Design};
use sv_syntax::NodeId;
use crate::support::routine_bodies::bodies;
use crate::support::class_roles::{class_methods, derives, is_library, role, Ctx, Index, Role};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

/// Field uses in one method: its own class's properties, and other
/// objects' properties per receiver (count, receiver class).
struct FieldUse<'d> {
    own: usize,
    foreign: BTreeMap<String, (usize, &'d Class)>,
}

/// Property `name` of `c` or a base: whether it is static.
fn property_static(d: &Design, c: &Class, name: &str) -> Option<bool> {
    std::iter::once(c)
        .chain(d.ancestors(c.id))
        .find_map(|k| k.properties.iter().find(|p| p.name == name))
        .map(|p| p.is_static)
}

/// Only objects with behaviour of their own count: not transactions,
/// config objects or sequences, not this class or its hierarchy.
fn envy_counts(d: &Design, me: &Class, t: &Class) -> bool {
    if t.id == me.id || is_library(d, t) {
        return false;
    }
    if matches!(role(d, t), Role::Item | Role::Config | Role::Sequence) {
        return false;
    }
    !derives(d, me.id, t.id) && !derives(d, t.id, me.id) && class_methods(t).next().is_some()
}

fn field_uses<'d>(ctx: &Ctx<'d, '_>, c: &Cst, n: NodeId, u: &mut FieldUse<'d>) {
    let d = ctx.d;
    match c.kind(n) {
        MemberExpr => {
            let Some(obj) = c.tree.children(n).next() else { return };
            let member = c.last_text(n);
            let recv = c.strip(obj);
            let bare = c.kind(recv) == NameExpr && c.tree.children(recv).next().is_none();
            if bare && matches!(c.tree.bare_text(recv).as_str(), "this" | "super") {
                // this.x is x itself.
                if property_static(d, ctx.cls, member) == Some(false) {
                    u.own += 1;
                }
                return;
            }
            if let Some(t) = ctx.class_of(c, recv) {
                if property_static(d, t, member).is_some() && envy_counts(d, ctx.cls, t) {
                    let e = u.foreign.entry(c.expr_text(recv)).or_insert((0, t));
                    e.0 += 1;
                    e.1 = t;
                    // The handle itself is not a use of this class's data.
                    if bare {
                        return;
                    }
                }
            }
        }
        NameExpr if c.tree.children(n).next().is_none() => {
            let name = c.tree.bare_text(n);
            let local = ctx.m.is_some_and(|m| m.locals.iter().any(|l| l.name == name) || m.arguments.iter().any(|a| a.name == name));
            if !local && property_static(d, ctx.cls, &name) == Some(false) {
                u.own += 1;
            }
            return;
        }
        _ => {}
    }
    for k in c.node_kids(n) {
        field_uses(ctx, c, k, u);
    }
}

pub(crate) fn feature_envy(d: &Design, csts: &[Cst], out: &mut Sink) {
    const MIN_FOREIGN: usize = 6;
    const EXEMPT: &[&str] = &[
        "new", "do_copy", "do_compare", "do_print", "do_pack", "do_unpack", "do_record", "copy", "compare", "convert2string",
        "build_phase", "connect_phase", "end_of_elaboration_phase",
    ];
    let ix = Index::new(csts);
    for (cls, m, c, body) in bodies(d, &ix) {
        if EXEMPT.contains(&m.name.as_str()) {
            continue;
        }
        let ctx = Ctx { d, cls, m: Some(m), td: Some(&ix.typedefs) };
        let mut u = FieldUse { own: 0, foreign: BTreeMap::new() };
        field_uses(&ctx, c, body, &mut u);
        let mut top: Option<(&String, &(usize, &Class))> = None;
        for e in &u.foreign {
            if top.map_or(true, |t| e.1 .0 > t.1 .0) {
                top = Some(e);
            }
        }
        let Some((key, &(count, t))) = top else { continue };
        if count < MIN_FOREIGN || count <= 2 * u.own {
            continue;
        }
        out.add(
            "SV-DSN-011",
            "FeatureEnvy",
            Severity::Info,
            format!(
                "Method '{}' of '{}' uses {count} fields of '{key}' (class '{}') and {} of its own. The logic belongs with that data: \
move it into '{}' as a method and call it from here.",
                m.name, cls.name, t.name, u.own, t.name
            ),
            m.loc,
        );
    }
}
