//! SV-DSN-003 (LowCohesion): Flags classes whose methods split into two or more
//! groups that share no fields and do not call each other (LCOM4 > 1): the
//! class is really several classes.

use crate::support::class_index::*;
use crate::support::class_metrics::each_class;
use crate::syntax::Cst;
use crate::{Severity, Sink};

pub(crate) fn low_cohesion(c: &Cst, s: &mut Sink) {
    let (min_groups, min_size) = (2, 2);
    each_class(c, |classes, cm| {
        let role = classes.role(cm.cls);
        if matches!(role, Role::Config | Role::Item | Role::RegModel) {
            return;
        }
        let groups: Vec<&Vec<String>> = cm.lcom_groups.iter().filter(|g| g.len() >= min_size).collect();
        if groups.len() < min_groups {
            return;
        }
        let desc = groups.iter().map(|g| format!("{{{}}}", g.join(", "))).collect::<Vec<_>>().join("; ");
        let cls = &classes.list[cm.cls];
        s.add(
            "SV-DSN-003",
            "LowCohesion",
            Severity::Info,
            format!(
                "{} '{}' has {} groups of methods that share no data: {desc}. Each group is a separate responsibility; consider moving them into their own classes.",
                role.name(),
                cls.name,
                groups.len()
            ),
            c.loc(cls.tok),
        );
    });
}
