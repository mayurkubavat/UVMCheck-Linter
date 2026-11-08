//! UVM-DEP-002 (UvmDeprecatedReportServer): Disallow direct singleton calls to
//! uvm_report_server::get_server() (IEEE 1800.2 Annex H).

use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::copies::{copies, routine_calls};

pub(crate) fn deprecated_report_server(c: &Cst, s: &mut Sink) {
    for (n, _, first) in routine_calls(c, &|x| x == "get_server") {
        if !c.text(n).contains("uvm_report_server") {
            continue;
        }
        for _ in 0..copies(c, n) {
            s.add(
                "UVM-DEP-002",
                "UvmDeprecatedReportServer",
                Severity::Warning,
                "Direct access via 'uvm_report_server::get_server()' is deprecated in IEEE 1800.2 § F.4.2 and Annex H. \
                 UVM report servers are now context-managed through uvm_coreservice_t. \
                 Replace with: 'uvm_coreservice_t cs = uvm_coreservice_t::get(); server = cs.get_report_server();'."
                    .into(),
                c.loc(first),
            );
        }
    }
}
