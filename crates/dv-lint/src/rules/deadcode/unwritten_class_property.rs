//! SV-DED-006 (UnwrittenClassProperty): Flags non-rand class properties that
//! lack an initializer, are read in methods, but never written or assigned.

use sv_syntax::SyntaxKind;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::ast::{class_properties};

pub(crate) fn unwritten_property(c: &Cst, s: &mut Sink) {
    for cls in c.nodes(ClassDecl) {
        let Some(name) = c.tree.child(cls, Name) else { continue };
        let class = c.tree.bare_text(name);
        let (props, u) = class_properties(c, cls, true);
        for (p, t, init) in props {
            if !init && u.read.contains(&p) && !u.written.contains(&p) {
                s.add(
                    "SV-DED-006",
                    "UnwrittenClassProperty",
                    Severity::Warning,
                    format!(
                        "Class property '{p}' is read in class '{class}' without an inline default initializer and is never assigned or initialized in any method. \
                         It will consistently evaluate to its uninitialized default value (0 or 'x), risking dead execution paths. \
                         Initialize the property at declaration or assign it inside the constructor 'new()'."
                    ),
                    c.loc(t),
                );
            }
        }
    }
}
