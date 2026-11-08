//! SV-DED-005 (UnreadClassProperty): Flags class properties that are assigned
//! or written to, but never read anywhere in the class.

use sv_syntax::SyntaxKind;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::ast::{class_properties};

pub(crate) fn unread_property(c: &Cst, s: &mut Sink) {
    for cls in c.nodes(ClassDecl) {
        let Some(name) = c.tree.child(cls, Name) else { continue };
        let class = c.tree.bare_text(name);
        let (props, u) = class_properties(c, cls, false);
        for (p, t, _) in props {
            if u.written.contains(&p) && !u.read.contains(&p) {
                s.add(
                    "SV-DED-005",
                    "UnreadClassProperty",
                    Severity::Info,
                    format!(
                        "Class property '{p}' is assigned or updated, but its value is never read in class '{class}'. Unread properties waste heap memory and indicate dead instrumentation or redundant state."
                    ),
                    c.loc(t),
                );
            }
        }
    }
}
