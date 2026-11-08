//! UVM-CFG-003 (UvmConfigDbGlobalWildcard): Calling uvm_config_db::set(null,
//! "*", ...) with a null context and a bare wildcard or empty scope triggers
//! global regex matching across every component in the testbench, severely
//! degrading simulation startup.

use crate::support::calls::{args, callee};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

pub(crate) fn config_db_wildcard(c: &Cst, s: &mut Sink) {
    for call in c.nodes(CallExpr) {
        let Some(f) = callee(c, call) else { continue };
        let text = c.text(f);
        if !text.contains("uvm_config_db") || !text.contains("::set") {
            continue;
        }
        let a = args(c, call);
        if a.len() < 2 {
            continue;
        }
        let arg = |x: Option<NodeId>| x.map(|n| c.trimmed(n)).unwrap_or_default();
        let (a0, a1) = (arg(a[0]), arg(a[1]));
        if a0 != "null" || (a1 != "\"*\"" && a1 != "\"\"") {
            continue;
        }
        let Some(t) = c.first_tok(call) else { continue };
        s.add(
            "UVM-CFG-003",
            "UvmConfigDbGlobalWildcard",
            Severity::Warning,
            format!(
                "Unbounded global config_db scope: uvm_config_db::set(null, {a1}, ...) sets configuration globally with no hierarchical context. \
                 This forces every component in the testbench hierarchy to perform regex string matching during build_phase, severely degrading simulation startup performance. Provide targeted scope (e.g. 'uvm_test_top.env.*')."
            ),
            c.loc(t),
        );
    }
}
