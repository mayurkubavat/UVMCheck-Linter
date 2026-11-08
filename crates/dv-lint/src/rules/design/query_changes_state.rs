//! SV-DSN-010 (QueryChangesState): Flags functions named get_*, is_* or has_*
//! that write class properties (command-query separation). get_next_* is
//! allowed.

use crate::support::class_index::{local_or_port, property_owner, written_names, Classes};
use crate::syntax::Cst;
use crate::{Severity, Sink};

pub(crate) fn query_changes_state(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    for i in classes.user() {
        let cls = &classes.list[i];
        for m in &cls.methods {
            let n = m.name.as_str();
            if m.is_task {
                continue;
            }
            if !(n.starts_with("get_") || n.starts_with("is_") || n.starts_with("has_")) || n.starts_with("get_next_") {
                continue;
            }
            let Some(body) = m.body else { continue };
            let mut changed: Option<String> = None;
            let g = crate::support::class_metrics::globals(c);
            let env = crate::support::class_metrics::Env::new(c, &classes, Some(i), Some(m.decl), &g);
            for w in written_names(c, &env, body, true) {
                if local_or_port(c, m.decl, &w) {
                    continue;
                }
                if property_owner(&classes, i, &w).is_none() {
                    continue;
                }
                if changed.as_ref().is_none_or(|x| w < *x) {
                    changed = Some(w);
                }
            }
            if let Some(sym) = changed {
                s.add(
                    "SV-DSN-010",
                    "QueryChangesState",
                    Severity::Info,
                    format!(
                        "'{}::{n}' reads like a query but changes '{sym}'. Callers, checks and assertions \
                         that call it twice see different results; split the update into its own \
                         method or rename it.",
                        cls.name
                    ),
                    c.loc(m.tok),
                );
            }
        }
    }
}
