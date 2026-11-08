//! UVM-REP-005 (LowVerbosityDebugReport): Flags UVM debug or trace messages
//! configured with low verbosity (UVM_LOW or UVM_NONE).

use dv_model::Design;
use sv_syntax::SyntaxKind;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::uvm_classes::{args, call_name, calls_in, int_value, string_value, unparen};

pub(crate) fn low_verbosity_debug(_d: &Design, csts: &[Cst], s: &mut Sink) {
    for c in csts {
        for n in calls_in(c, c.tree.root) {
            let name = call_name(c, n);
            if !(name.contains("uvm_report_info") || name.contains("uvm_info")) {
                continue;
            }
            let a = args(c, n);
            if a.len() < 3 {
                continue;
            }
            let (Some(id), Some(verb)) = (a[0], a[2]) else { continue };
            let idv = string_value(c, id).unwrap_or_default();
            let msg = a[1].and_then(|m| string_value(c, m)).unwrap_or_default();
            let (ui, um) = (idv.to_ascii_uppercase(), msg.to_ascii_uppercase());
            let debug = ui.contains("DEBUG") || ui.contains("TRACE") || ui.contains("DUMP") || um.contains("DEBUG") || um.contains("TRACE");
            if !debug {
                continue;
            }
            let v = unparen(c, verb);
            let mut verb_name = c.text(verb);
            let low = match c.kind(v) {
                NameExpr | ScopedExpr => {
                    let sym = c.last_text(v).to_string();
                    let l = sym == "UVM_LOW" || sym == "UVM_NONE";
                    if l {
                        verb_name = sym;
                    }
                    l
                }
                Literal => int_value(&c.trimmed(v)).is_some_and(|x| x <= 100),
                _ => verb_name.contains("UVM_LOW") || verb_name.contains("UVM_NONE"),
            };
            if !low {
                continue;
            }
            let Some(t) = c.first_tok(n) else { continue };
            s.add(
                "UVM-REP-005",
                "LowVerbosityDebugReport",
                Severity::Warning,
                format!(
                    "UVM report with debug/trace content (ID: '{idv}') is configured with verbosity '{verb_name}'. In UVM logging best practices \
                     (IEEE 1800.2 § 6.3), debug and trace messages must be configured with \
                     UVM_HIGH or UVM_DEBUG to prevent flooding simulation logs during regression runs."
                ),
                c.loc(t),
            );
        }
    }
}
