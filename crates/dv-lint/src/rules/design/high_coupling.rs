//! SV-DSN-004 (HighCoupling): Flags classes other than envs and tests that
//! reference more than 10 distinct user class types (CK coupling between
//! objects, PMD CouplingBetweenObjects).

use crate::support::class_index::*;
use crate::support::class_metrics::each_class;
use crate::syntax::Cst;
use crate::{Severity, Sink};

pub(crate) fn high_coupling(c: &Cst, s: &mut Sink) {
    let limit = crate::options::get("SV-DSN-004", "max_cbo", 10) as _;
    each_class(c, |classes, cm| {
        let role = classes.role(cm.cls);
        if matches!(role, Role::Env | Role::Test | Role::RegModel) || cm.cbo <= limit {
            return;
        }
        let cls = &classes.list[cm.cls];
        s.add(
            "SV-DSN-004",
            "HighCoupling",
            Severity::Info,
            format!(
                "{} '{}' depends on {} classes (limit {limit}): {}. A change to any of them can break it; depend on the item, the config and \
                 interfaces, and reach the rest through TLM.",
                role.name(),
                cls.name,
                cm.cbo,
                cm.coupled_to.join(", ")
            ),
            c.loc(cls.tok),
        );
    });
}
