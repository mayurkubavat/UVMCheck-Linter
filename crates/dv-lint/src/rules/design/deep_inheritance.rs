//! SV-DSN-005 (DeepInheritance): Flags classes with more than 3 user-defined
//! ancestors above the UVM base class (CK depth of inheritance tree, Pylint
//! too-many-ancestors).

use crate::support::class_metrics::each_class;
use crate::syntax::Cst;
use crate::{Severity, Sink};

pub(crate) fn deep_inheritance(c: &Cst, s: &mut Sink) {
    let limit = crate::options::get("SV-DSN-005", "max_depth", 3) as _;
    each_class(c, |classes, cm| {
        if cm.dit <= limit {
            return;
        }
        let cls = &classes.list[cm.cls];
        let mut chain = cls.name.clone();
        for a in &cm.ancestry {
            chain.push_str(&format!(" : {a}"));
        }
        if let Some(b) = classes.uvm_base_name(cm.cls) {
            chain.push_str(&format!(" : {b}"));
        }
        s.add(
            "SV-DSN-005",
            "DeepInheritance",
            Severity::Info,
            format!(
                "'{}' has {} user-defined ancestors (limit {limit}): {chain}. It is hard to tell which override of each phase runs; prefer composition or \
                 config objects over another level.",
                cls.name, cm.dit
            ),
            c.loc(cls.tok),
        );
    });
}
