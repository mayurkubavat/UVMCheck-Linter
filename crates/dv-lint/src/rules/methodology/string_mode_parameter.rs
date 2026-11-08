//! SV-CPL-001 (StringModeParameter): Flags routines whose string argument is
//! compared against two or more literals to choose what to do, as in
//! do_op("read").

use std::collections::HashSet;
use dv_model::Design;
use crate::support::overrides::overrides_base;
use crate::support::class_roles::Index;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;
use crate::support::string_codes::{all_uvm_access_policies, dispatches, literal_list, user_routines};

pub(crate) fn string_mode_parameter(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for r in user_routines(d, &ix) {
        if let (Some(cls), Some(m)) = (r.cls, r.m) {
            if overrides_base(d, cls, &m.name) {
                continue;
            }
        }
        let name = match r.m {
            Some(m) => m.name.clone(),
            None => r.c.tree.child(r.decl, Name).map(|n| r.c.tree.bare_text(n)).unwrap_or_default(),
        };
        let mut reported = HashSet::new();
        for x in dispatches(d, &ix, &r, true, 2) {
            if all_uvm_access_policies(&x.lits) || !reported.insert(x.sym.sym.clone()) {
                continue;
            }
            out.add(
                "SV-CPL-001",
                "StringModeParameter",
                Severity::Warning,
                format!(
                    "'{name}' chooses its behaviour from the string argument '{}' ({}). Every caller has to spell these values exactly, \
and a typo is not caught. Take an enum argument, or split the routine into one routine per mode.",
                    x.sym.name,
                    literal_list(&x.lits)
                ),
                x.sym.loc,
            );
        }
    }
}
