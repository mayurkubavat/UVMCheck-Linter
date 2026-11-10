//! UVM-TLM-003 (UvmTlmPortSuffix): Enforces standard UVM TLM port suffix naming
//! conventions (_ap for analysis ports, _imp for imps, _export for exports,
//! _port for ports, _fifo for TLM FIFOs).

use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind::*;

pub(crate) fn tlm_port_suffix(c: &Cst, s: &mut Sink) {
    for n in c.nodes(DataDecl) {
        let Some(ty) = c.tree.child(n, DataType) else { continue };
        let Some(first) = c.first_tok(ty) else { continue };
        let base = c.text_of(first);
        let (suffix, desc) = if base == "uvm_analysis_port" {
            ("_ap", "'_ap' or '_port'")
        } else if base.contains("analysis_imp") || base.ends_with("_imp") {
            ("_imp", "'_imp'")
        } else if base.contains("analysis_export") || base.ends_with("_export") {
            ("_export", "'_export' or '_exp'")
        } else if base.contains("tlm_fifo") || base.ends_with("_fifo") {
            ("_fifo", "'_fifo'")
        } else if base.contains("uvm_") && base.ends_with("_port") {
            ("_port", "'_port'")
        } else {
            continue;
        };
        for d in c.tree.children(n).filter(|&d| c.kind(d) == Declarator) {
            let Some(nm) = c.tree.child(d, Name) else { continue };
            let var = c.tree.bare_text(nm);
            if var.is_empty() {
                continue;
            }
            let ok = match suffix {
                "_ap" => var.ends_with("_ap") || var.ends_with("_port") || var == "ap",
                "_export" => var.ends_with("_export") || var.ends_with("_exp") || var == "export" || var == "exp",
                "_fifo" => var.ends_with("_fifo") || var == "fifo",
                "_imp" => var.ends_with("_imp") || var == "imp",
                x => var.ends_with(x),
            };
            if ok {
                continue;
            }
            let Some(t) = c.first_tok(nm) else { continue };
            s.add(
                "UVM-TLM-003",
                "UvmTlmPortSuffix",
                Severity::Warning,
                format!(
                    "TLM object '{var}' of type '{base}' violates naming conventions: expected suffix {desc}. \
                     Standardized TLM suffixes prevent confusion between ports, exports, and imps during connect_phase."
                ),
                c.loc(t),
            );
        }
    }
}
