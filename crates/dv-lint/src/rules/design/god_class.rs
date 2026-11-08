//! SV-DSN-002 (GodClass): Flags classes that are very complex (WMC >= 47), use
//! many other classes' data (ATFD > 5) and have low cohesion (TCC < 1/3): PMD's
//! GodClass detection strategy.

use crate::support::class_index::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::class_metrics::{each_class};

pub(crate) fn god_class(c: &Cst, s: &mut Sink) {
    let (wmc_limit, atfd_limit, tcc_percent) = (47, 5, 33.0);
    each_class(c, |classes, cm| {
        let role = classes.role(cm.cls);
        if role == Role::RegModel {
            return;
        }
        if cm.wmc < wmc_limit || cm.atfd <= atfd_limit || cm.tcc * 100.0 >= tcc_percent {
            return;
        }
        let cls = &classes.list[cm.cls];
        s.add(
            "SV-DSN-002",
            "GodClass",
            Severity::Warning,
            format!(
                "{} '{}' is a god class: weighted method complexity {}, uses {} fields of other classes, cohesion (TCC) {:.2}. \
                 It does several jobs at once; split it, for example into predictor, comparator and \
                 coverage parts.",
                role.name(),
                cls.name,
                cm.wmc,
                cm.atfd,
                cm.tcc
            ),
            c.loc(cls.tok),
        );
    });
}
