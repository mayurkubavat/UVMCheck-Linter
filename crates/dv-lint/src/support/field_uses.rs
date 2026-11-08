//! Where each config_db field name is set and got.

use dv_model::{Design, Loc};

use crate::support::routine_bodies::{bodies, calls_in, is_config_db_call, Calls};

use crate::support::class_roles::Index;

use crate::syntax::Cst;

/// Field names with where each is used.
pub(crate) type FieldUses = Vec<(String, Loc)>;

pub(crate) fn field_uses(d: &Design, csts: &[Cst]) -> (FieldUses, FieldUses) {
    let ix = Index::new(csts);
    let (mut sets, mut gets) = (Vec::new(), Vec::new());
    for (_, m, c, body) in bodies(d, &ix) {
        let calls = Calls::new(m);
        for call in calls_in(c, body) {
            let is_set = is_config_db_call(&calls, c, call, "set");
            let is_get = is_config_db_call(&calls, c, call, "get");
            let args = c.call_args(call);
            if (!is_set && !is_get) || args.len() < 3 {
                continue;
            }
            let Some(field) = c.arg_expr(args[2]).and_then(|e| c.string_value(e)) else { continue };
            if field.is_empty() || field == "*" {
                continue;
            }
            if is_set { &mut sets } else { &mut gets }.push((field, c.loc_of(call)));
        }
    }
    (sets, gets)
}
