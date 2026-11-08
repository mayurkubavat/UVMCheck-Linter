//! UVM-RUS-004 (AbsoluteConfigDbPath): Flags uvm_config_db calls in classes
//! whose path starts at uvm_test_top.

use dv_model::Design;
use crate::support::class_roles::{role, role_name, Index, Role};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::routine_bodies::{Calls, bodies, calls_in, is_config_db_call};

pub(crate) fn absolute_config_db_path(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for (cls, m, c, body) in bodies(d, &ix) {
        let r = role(d, cls);
        let calls = Calls::new(m);
        for call in calls_in(c, body) {
            let is_set = is_config_db_call(&calls, c, call, "set");
            let args = c.call_args(call);
            if (!is_set && !is_config_db_call(&calls, c, call, "get")) || args.len() < 2 {
                continue;
            }
            let path = c.arg_expr(args[1]).and_then(|a| c.string_value(a)).unwrap_or_default();
            let p = path.trim_start_matches(['*', '.']);
            if !p.starts_with("uvm_test_top") {
                continue;
            }
            let null_context = c.arg_expr(args[0]).is_some_and(|a| c.is_null(a));
            if null_context && r == Role::Test {
                continue;
            }
            let msg = if null_context {
                format!(
                    "{} '{}' uses the absolute config_db path \"{}\". The class then only works at that one place in one test's \
hierarchy. Use 'this' as the context and a path relative to it.",
                    role_name(r),
                    cls.name,
                    path
                )
            } else {
                format!(
                    "config_db {}() in '{}' combines a non-null context with the absolute path \"{}\". UVM appends the path to the \
context's full name, so it can never match. Pass null as the context, or make the path relative.",
                    if is_set { "set" } else { "get" },
                    cls.name,
                    path
                )
            };
            out.add("UVM-RUS-004", "AbsoluteConfigDbPath", Severity::Warning, msg, c.loc_of(call));
        }
    }
}
