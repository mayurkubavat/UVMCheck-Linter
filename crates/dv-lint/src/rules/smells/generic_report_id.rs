//! SV-SML-003 (GenericReportId): Disallow generic single-word report IDs
//! ('ERR', 'INFO', 'DEBUG') in UVM reporting macros.

use sv_syntax::SyntaxKind;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::statements::{Units, call_args, call_name, report, start, string_literal};

pub(crate) fn generic_report_id(c: &Cst, s: &mut Sink) {
    let units = Units::new(c);
    for call in c.nodes(CallExpr) {
        let Some(name) = call_name(c, call) else { continue };
        let report_call = [
            "uvm_report_info",
            "uvm_report_warning",
            "uvm_report_error",
            "uvm_report_fatal",
            "uvm_info",
            "uvm_error",
            "uvm_warning",
            "uvm_fatal",
        ]
        .iter()
        .any(|k| name.contains(k));
        if !report_call {
            continue;
        }
        let Some(Some(id)) = call_args(c, call).into_iter().next() else {
            continue;
        };
        let Some(val) = string_literal(c, id) else { continue };
        let upper = val.to_ascii_uppercase();
        if matches!(
            upper.as_str(),
            "ERR" | "ERROR" | "INFO" | "WARN" | "WARNING" | "DEBUG" | "FATAL" | "TEST" | "TB" | "MSG" | "TEMP" | ""
        ) {
            report(
                s,
                units.times(c, call),
                "SV-SML-003",
                "GenericReportId",
                Severity::Info,
                format!(
                    "Generic or cryptic UVM report ID '{val}'. Using generic single-word report IDs prevents targeted log filtering and severity overrides via '+uvm_set_severity' and 'uvm_report_catcher'. \
                     Use descriptive, feature-scoped identifiers (e.g. 'AXI_SLV_TIMEOUT', 'PCIE_TLP_CRC_ERR')."
                ),
                start(c, id),
            );
        }
    }
}
