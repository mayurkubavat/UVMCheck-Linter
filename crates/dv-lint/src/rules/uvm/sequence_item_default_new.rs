//! UVM-SEQ-008 (SequenceItemDefaultNew): Requires sequence item constructor
//! new() to provide a default value for the string name argument to support UVM
//! factory creation and cloning.

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;
use crate::support::uvm_syntax::{base_name_node, class_routines, decl_name, name_node, routine_last_name};

pub(crate) fn sequence_item_default_new(c: &Cst, s: &mut Sink) {
    for cls in c.nodes(ClassDecl) {
        let Some(base) = base_name_node(c, cls) else { continue };
        let base = c.last_text(base);
        if base != "uvm_sequence_item" && !base.contains("sequence_item") {
            continue;
        }
        let class = decl_name(c, cls);
        for f in class_routines(c, cls) {
            if c.kind(f) != FunctionDecl || routine_last_name(c, f) != "new" {
                continue;
            }
            let Some(ports) = c.tree.child(f, PortList) else { continue };
            let Some(port) = c.tree.children(ports).find(|&p| c.kind(p) == Port) else { continue };
            let Some(d) = c.tree.child(port, Declarator) else { continue };
            if c.tok(d, "=").is_some() {
                continue;
            }
            let Some(nm) = name_node(c, d) else { continue };
            let Some(at) = c.first_tok(nm) else { continue };
            let port_name = c.tree.bare_text(nm);
            s.add(
                "UVM-SEQ-008",
                "SequenceItemDefaultNew",
                Severity::Warning,
                format!(
                    "Constructor 'new' of sequence item class '{class}' defines argument '{port_name}' without a default value. In UVM, sequence item constructors must provide a default string name (e.g. 'string name = \"{class}\"') to support factory creation (`uvm_object_utils) and .clone() / .copy() operations."
                ),
                c.loc(at),
            );
        }
    }
}
