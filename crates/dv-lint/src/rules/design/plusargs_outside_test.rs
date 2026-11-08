//! UVM-CFG-006 (PlusargsOutsideTest): Flags $test$plusargs, $value$plusargs and
//! uvm_cmdline_processor reads in classes other than tests and config objects.

use crate::support::class_index::{calls, Classes, Role};
use crate::support::statements::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};

pub(crate) fn plusargs_outside_test(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    for i in classes.user() {
        let role = classes.role(i);
        if role == Role::Test || role == Role::Config {
            continue;
        }
        let cls = &classes.list[i];
        let g = crate::support::class_metrics::globals(c);
        for m in &cls.methods {
            let Some(body) = m.body else { continue };
            let env = crate::support::class_metrics::Env::new(c, &classes, Some(i), Some(m.decl), &g);
            for call in calls(c, body) {
                let name = call_name(c, call).unwrap_or("");
                let read = match name {
                    "$test$plusargs" | "$value$plusargs" => true,
                    "get_arg_value" | "get_arg_values" | "get_arg_matches" | "get_plusargs" => {
                        env.callee(call).is_some_and(|(k, _)| classes.list[k].library)
                    }
                    _ => false,
                };
                if !read {
                    continue;
                }
                s.add(
                    "UVM-CFG-006",
                    "PlusargsOutsideTest",
                    Severity::Warning,
                    format!(
                        "{} '{}' reads the command line with {name}() in {}(). Hidden switches make runs hard to \
                         reproduce and the class hard to reuse; read plusargs once in the test or a \
                         config object and pass the values down through the config.",
                        role.name(),
                        cls.name,
                        m.name
                    ),
                    start(c, call),
                );
            }
        }
    }
}
