//! SV-MET-006 (LargeClass): Flags classes with more than 30 properties or 30
//! methods; config objects and sequence items may have 80 properties, register
//! models are exempt (PMD TooManyFields and TooManyMethods, Pylint
//! R0902/R0904).

use crate::support::class_index::*;
use crate::support::class_metrics::each_class;
use crate::syntax::Cst;
use crate::{Severity, Sink};

pub(crate) fn large_class(c: &Cst, s: &mut Sink) {
    let (max_fields, max_data_fields, max_methods) = (
        crate::options::get("SV-MET-006", "max_fields", 30) as _,
        crate::options::get("SV-MET-006", "max_data_fields", 80) as _,
        crate::options::get("SV-MET-006", "max_methods", 30) as _,
    );
    each_class(c, |classes, cm| {
        let role = classes.role(cm.cls);
        if role == Role::RegModel {
            return;
        }
        let data = matches!(role, Role::Config | Role::Item);
        let field_limit = if data { max_data_fields } else { max_fields };
        let mut what = String::new();
        if cm.fields > field_limit {
            what = format!("{} properties (limit {field_limit})", cm.fields);
        }
        if cm.methods > max_methods {
            if !what.is_empty() {
                what.push_str(" and ");
            }
            what.push_str(&format!("{} methods (limit {max_methods})", cm.methods));
        }
        if what.is_empty() {
            return;
        }
        let cls = &classes.list[cm.cls];
        s.add(
            "SV-MET-006",
            "LargeClass",
            Severity::Info,
            format!(
                "{} '{}' has {what}. Large classes are hard to review and usually hold several responsibilities; split \
                 out sub-configs, helpers or separate components.",
                role.name(),
                cls.name
            ),
            c.loc(cls.tok),
        );
    });
}
