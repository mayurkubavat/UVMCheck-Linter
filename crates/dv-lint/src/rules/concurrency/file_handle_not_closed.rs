//! SV-RSC-002 (FileHandleNotClosed): Flags $fopen calls in classes that never
//! call $fclose, which loses buffered trace data and exhausts file handles in
//! long regressions (Sonar S2095).

use crate::support::procedural::{calls_in, start};
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn file_not_closed(c: &Cst, s: &mut Sink) {
    for cls in user_classes(c) {
        let mut opens = Vec::new();
        let mut closes = false;
        for m in class_methods(c, cls) {
            let Some(body) = c.tree.child(m, Body) else { continue };
            for call in calls_in(c, body) {
                let n = call_parts(c, call).map(|p| p.1).unwrap_or_default();
                if n == "$fopen" {
                    opens.push(call);
                } else if n == "$fclose" {
                    closes = true;
                }
            }
        }
        if closes {
            continue;
        }
        let cname = decl_name(c, cls);
        for call in opens {
            let Some(loc) = start(c, call) else { continue };
            s.add(
                "SV-RSC-002",
                "FileHandleNotClosed",
                Severity::Warning,
                format!(
                    "'{cname}' opens a file with {} but never calls $fclose. Close it in final_phase or report_phase so buffered output is flushed and the handle is released.",
                    norm_text(c, call)
                ),
                loc,
            );
        }
    }
}
