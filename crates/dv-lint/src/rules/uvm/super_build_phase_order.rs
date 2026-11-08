//! UVM-PHS-005 (SuperBuildPhaseOrder): Enforces that super.build_phase(phase)
//! is the very first procedural statement in build_phase.

use dv_model::Design;
use sv_syntax::SyntaxKind;
use crate::support::uvm_syntax::{
    body_items, routine_last_name,
};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::uvm_classes::{Ix, comp_decl, routines_in};

fn strip_comments_and_ws(input: &str) -> String {
    let b = input.as_bytes();
    let mut out = Vec::new();
    let (mut line, mut block) = (false, false);
    let mut i = 0;
    while i < b.len() {
        if line {
            if b[i] == b'\n' {
                line = false;
            }
        } else if block {
            if b[i] == b'*' && i + 1 < b.len() && b[i + 1] == b'/' {
                block = false;
                i += 1;
            }
        } else if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            line = true;
            i += 1;
        } else if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            block = true;
            i += 1;
        } else if !matches!(b[i], b' ' | b'\t' | b'\n' | b'\r') {
            out.push(b[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(crate) fn super_build_phase_order(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for c in csts {
        for (cls, f) in routines_in(&ix, c, comp_decl) {
            if routine_last_name(c, f) != "build_phase" {
                continue;
            }
            let mut seen = false;
            for item in body_items(c, f) {
                if matches!(c.kind(item), DataDecl | EmptyStmt) {
                    continue;
                }
                if strip_comments_and_ws(&c.text(item)).contains("super.build_phase") {
                    if seen {
                        let Some(t) = c.first_tok(item) else { break };
                        s.add(
                            "UVM-PHS-005",
                            "SuperBuildPhaseOrder",
                            Severity::Warning,
                            format!(
                                "'super.build_phase(phase)' in class '{cls}' is called after preceding procedural statements. In UVM, build_phase executes top-down; \
                                 calling super.build_phase(phase) as the first statement ensures base-class configurations and sub-components are initialized before derived configuration."
                            ),
                            c.loc(t),
                        );
                    }
                    break;
                }
                seen = true;
            }
        }
    }
}
