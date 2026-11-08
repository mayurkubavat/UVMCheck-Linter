//! SV-DSN-007 (EmptyOverrideOfConcrete): Flags an empty override of a user
//! base-class method that has a body, which silently switches the inherited
//! behavior off (Fowler 'refused bequest', Liskov substitution).

use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::class_index::{Classes, count_statements};

pub(crate) fn empty_override(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    for i in classes.user() {
        let cls = &classes.list[i];
        for m in &cls.methods {
            let Some((k, Ok(bi))) = classes.override_of(i, &m.name) else {
                continue;
            };
            if classes.list[k].library {
                continue;
            }
            let Some(body) = m.body else { continue };
            if count_statements(c, body) != 0 {
                continue;
            }
            let base = &classes.list[k].methods[bi];
            if base.body.map(|b| count_statements(c, b)).unwrap_or(0) == 0 {
                continue;
            }
            s.add(
                "SV-DSN-007",
                "EmptyOverrideOfConcrete",
                Severity::Warning,
                format!(
                    "'{}::{}' is empty and replaces '{}::{}', which does work. Subclasses that switch inherited \
                     behavior off break code that relies on the base contract; add a config knob \
                     in the base class instead.",
                    cls.name, m.name, classes.list[k].name, base.name
                ),
                c.loc(m.tok),
            );
        }
    }
}
