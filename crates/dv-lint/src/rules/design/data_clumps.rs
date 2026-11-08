//! SV-DUP-006 (DataClumps): Flags the same group of min_args (default 4) or
//! more arguments (same names and types) declared by min_routines (default 3)
//! or more routines.

use crate::support::class_index::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::BTreeMap;
use sv_syntax::NodeId;

/// slang's `Type::toString()` of a port type, as far as grouping needs.
fn type_text(c: &Cst, ty: Option<NodeId>) -> String {
    match ty {
        Some(t) => c.tree.bare_text(t),
        None => "logic".into(),
    }
}

pub(crate) fn data_clumps(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    let (min_args, min_routines) = (4, 3);
    let mut groups: BTreeMap<String, Vec<(usize, usize)>> = BTreeMap::new();
    for i in classes.user() {
        for (mi, m) in classes.list[i].methods.iter().enumerate() {
            if classes.override_of(i, &m.name).is_some() || m.ports.len() < min_args {
                continue;
            }
            let mut parts: Vec<String> = m.ports.iter().map(|p| format!("{} {}", type_text(c, p.ty), p.name)).collect();
            parts.sort();
            let key: String = parts.iter().map(|p| format!("{p};")).collect();
            groups.entry(key).or_default().push((i, mi));
        }
    }
    for routines in groups.values() {
        if routines.len() < min_routines {
            continue;
        }
        let mut sorted = routines.clone();
        let pos = |&(i, m): &(usize, usize)| {
            let l = c.loc(classes.list[i].methods[m].tok);
            let l = l.expansion.map(|(f, line)| (f, line, 0)).unwrap_or((l.file, l.line, l.column));
            (l, classes.list[i].methods[m].tok)
        };
        sorted.sort_by_key(pos);
        let mut names = String::new();
        for &(i, m) in sorted.iter().skip(1).take(5) {
            if !names.is_empty() {
                names.push_str(", ");
            }
            names.push_str(&format!("{}::{}", classes.list[i].name, classes.list[i].methods[m].name));
        }
        if sorted.len() > 6 {
            names.push_str(", ...");
        }
        let (fi, fm) = sorted[0];
        let first = &classes.list[fi].methods[fm];
        s.add(
            "SV-DUP-006",
            "DataClumps",
            Severity::Info,
            format!(
                "'{}' and {} other routine(s) ({names}) take the same {} arguments. Pass one transaction or struct that holds them.",
                first.name,
                sorted.len() - 1,
                first.ports.len()
            ),
            c.loc(first.tok),
        );
    }
}
