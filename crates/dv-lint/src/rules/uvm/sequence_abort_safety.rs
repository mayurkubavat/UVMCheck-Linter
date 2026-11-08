//! UVM-SEQ-004 (SequenceAbortSafety): Detects calls to start_item() in UVM
//! sequences that can bypass finish_item() via early return or break
//! statements.

use crate::support::uvm_syntax::{
    class_routines, decl_name, extends_clause, name_loc, name_node, routine_first_name, routine_text,
    strip_line_comments,
};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn sequence_abort_safety(c: &Cst, s: &mut Sink) {
    for cls in c.nodes(ClassDecl) {
        let name = decl_name(c, cls);
        let ext = extends_clause(c, cls).map(|e| c.text(e)).unwrap_or_default();
        if !(ext.contains("uvm_sequence") || name.contains("sequence") || name.contains("seq")) {
            continue;
        }
        for f in class_routines(c, cls) {
            if name_node(c, f).is_none() {
                continue;
            }
            let text = strip_line_comments(&routine_text(c, f));
            let mut start = 0;
            while let Some(at) = text[start..].find("start_item").map(|i| i + start) {
                let finish = text[at..].find("finish_item").map(|i| i + at);
                if let Some(ret) = text[at..].find("return").map(|i| i + at) {
                    let after = text[ret + 6..].find(|ch| ch != ' ' && ch != '\t').map(|i| i + ret + 6);
                    if after.is_some_and(|a| text.as_bytes()[a] == b';') && finish.is_none_or(|f| ret < f) {
                        if let Some(loc) = name_loc(c, f) {
                            s.add(
                                "UVM-SEQ-004",
                                "SequenceAbortSafety",
                                Severity::Error,
                                format!(
                                    "Task '{}' in sequence class '{name}' calls start_item() but contains an early 'return;' before finish_item(). \
                                     Aborting a sequence handshake without calling finish_item() permanently leaves the driver-sequencer pipeline in an un-granted deadlock.",
                                    routine_first_name(c, f)
                                ),
                                loc,
                            );
                        }
                        break;
                    }
                }
                start = at + 10;
            }
        }
    }
}
