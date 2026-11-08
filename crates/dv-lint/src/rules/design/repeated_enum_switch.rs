//! SV-DSN-009 (RepeatedEnumSwitch): Flags an enum that is switched on with case
//! in three or more classes, a sign that behavior per kind belongs in a
//! polymorphic item or policy class (Fowler 'repeated switches').

use crate::support::class_index::*;
use crate::support::class_metrics::{globals, Env};
use crate::support::statements::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::{BTreeMap, HashMap, HashSet};
use sv_syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

/// Enum typedef names.
fn enum_types(c: &Cst) -> HashSet<String> {
    let mut out = HashSet::new();
    for t in c.nodes(TypedefDecl) {
        let (Some(nm), Some(ty)) = (c.tree.child(t, Name), c.tree.child(t, DataType)) else {
            continue;
        };
        if c.first_tok(ty).is_some_and(|k| c.text_of(k) == "enum") {
            out.insert(c.tree.bare_text(nm));
        }
    }
    out
}

pub(crate) fn repeated_enum_switch(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    let g = globals(c);
    let enums = enum_types(c);
    let mut sites: HashMap<String, Vec<(usize, NodeId)>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for i in classes.user() {
        let mut first: BTreeMap<String, NodeId> = BTreeMap::new();
        for m in &classes.list[i].methods {
            let Some(body) = m.body else { continue };
            let env = Env::new(c, &classes, Some(i), Some(m.decl), &g);
            for st in crate::support::class_index::all_stmts(c, body) {
                if stmt(c, st) != Some(St::Case) {
                    continue;
                }
                let Some(sel) = case_parts(c, st).sel else { continue };
                let Some(t) = env.expr_type(sel) else { continue };
                if t.array || !enums.contains(&t.name) {
                    continue;
                }
                first.entry(t.name).or_insert(st);
            }
        }
        for (ty, st) in first {
            let v = sites.entry(ty.clone()).or_default();
            if v.is_empty() {
                order.push(ty);
            }
            v.push((i, st));
        }
    }
    let min = 3;
    for ty in order {
        let v = &sites[&ty];
        if v.len() < min {
            continue;
        }
        let names = v.iter().map(|(i, _)| classes.list[*i].name.as_str()).collect::<Vec<_>>().join(", ");
        for (i, st) in v {
            s.add(
                "SV-DSN-009",
                "RepeatedEnumSwitch",
                Severity::Info,
                format!(
                    "'{}' switches on enum '{ty}', as do {} other classes ({names}). Every new \
                     value needs an edit in each of them; consider moving the per-kind behavior \
                     into the item or a policy class.",
                    classes.list[*i].name,
                    v.len() - 1
                ),
                start(c, *st),
            );
        }
    }
}
