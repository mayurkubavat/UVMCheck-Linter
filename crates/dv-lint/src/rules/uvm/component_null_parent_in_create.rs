//! UVM-STR-005 (ComponentNullParentInCreate): Flags UVM component factory
//! create calls passing 'null' as the parent handle, creating orphaned
//! hierarchy nodes.

use crate::support::calls::{args, callee};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

/// An ordered (not named) argument's expression.
fn ordered(c: &Cst, a: Option<NodeId>) -> Option<NodeId> {
    let a = a?;
    if c.kind(a) != Arg || c.tok(a, ".").is_some() {
        return None;
    }
    c.tree.children(a).next()
}

pub(crate) fn null_parent_create(c: &Cst, s: &mut Sink) {
    for call in c.nodes(CallExpr) {
        let Some(f) = callee(c, call) else { continue };
        let text = c.text(f);
        if !text.contains("::type_id::create") && !text.contains(".create") {
            continue;
        }
        let a = args(c, call);
        let Some(e) = a.get(1).and_then(|&x| ordered(c, x)) else { continue };
        if c.trimmed(e) != "null" {
            continue;
        }
        let whole = c.text(call);
        let call_text = whole.rsplit('\n').next().unwrap_or("").trim_matches([' ', '\t', '\r']);
        let Some(t) = c.first_tok(e) else { continue };
        s.add(
            "UVM-STR-005",
            "ComponentNullParentInCreate",
            Severity::Warning,
            format!(
                "Component factory creation '{call_text}' passes 'null' as the parent component handle. In UVM methodology (IEEE 1800.2 § 4.3 and \
                 IEEE 1800.2 § 4.3), sub-components must specify 'this' as parent to properly integrate into \
                 the component hierarchy. Passing 'null' creates an orphaned component excluded from phase \
                 traversal, topology reporting, and hierarchical uvm_config_db lookup."
            ),
            c.loc(t),
        );
    }
}
