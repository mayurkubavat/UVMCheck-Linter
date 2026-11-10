//! RAL-ACC-002 (MissingBackdoorAccessPathDefinition): Flags RAL backdoor path
//! configuration routines called with an empty string or null path.

use crate::support::uvm_syntax::{callee, first_ordered};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind::*;

pub(crate) fn backdoor_path(c: &Cst, s: &mut Sink) {
    for n in c.nodes(CallExpr) {
        let method = callee(c, n);
        if !matches!(method, "add_hdl_path" | "set_hdl_path_root" | "add_hdl_path_slice" | "set_default_hdl_path") {
            continue;
        }
        let Some(expr) = first_ordered(c, n) else { continue };
        let path = c.trimmed(expr);
        if path == "\"\"" || path == "null" {
            let Some(at) = c.first_tok(expr) else { continue };
            s.add(
                "RAL-ACC-002",
                "MissingBackdoorAccessPathDefinition",
                Severity::Warning,
                format!(
                    "RAL backdoor path configuration '{method}(\"\")' specifies an empty or null HDL path string. \
                     In UVM RAL architecture (IEEE 1800.2 § 18.8), register backdoor read/write operations \
                     require a non-empty HDL root path to resolve target register signals within the simulation hierarchy."
                ),
                c.loc(at),
            );
        }
    }
}
