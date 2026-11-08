//! UVM-REP-009 (InfoAtNoneVerbosity): Flags `uvm_info messages at UVM_NONE
//! verbosity, which cannot be filtered with +UVM_VERBOSITY.

use crate::support::literals::ast_times;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn info_at_none(c: &Cst, s: &mut Sink) {
    for call in c.nodes(CallExpr) {
        if crate::support::class_items::call_name(c, call) != Some("uvm_report_info") {
            continue;
        }
        let args = c.call_args(call);
        let Some(v) = c.arg_expr(args.get(2).copied().flatten()) else { continue };
        let v = c.strip(v);
        let none = match c.kind(v) {
            NameExpr | ScopedExpr => c.last_text(v) == "UVM_NONE",
            Literal => {
                let t: String = c.toks(v).map(|t| c.text_of(t)).collect();
                let digits = t.rsplit(['d', 'D', 'h', 'H', 'b', 'B', 'o', 'O', '\'']).next().unwrap_or("");
                !digits.is_empty() && digits.chars().all(|ch| ch == '0' || ch == '_')
            }
            _ => false,
        };
        if !none {
            continue;
        }
        let Some(first) = c.first_tok(call) else { continue };
        let loc = c.loc_at(c.tree.tokens[first as usize].origin);
        for _ in 0..ast_times(c, call) {
            s.add(
                "UVM-REP-009",
                "InfoAtNoneVerbosity",
                Severity::Info,
                "Informational message at UVM_NONE: it prints at every verbosity, +UVM_VERBOSITY cannot silence it, and it floods regression logs. Use UVM_LOW for messages that should normally show, and keep UVM_NONE for the few lines every run must print (a test's final verdict)."
                    .to_string(),
                loc,
            );
        }
    }
}
