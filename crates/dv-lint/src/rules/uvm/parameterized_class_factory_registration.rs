//! UVM-FAC-004 (ParameterizedClassFactoryRegistration): Parameterized UVM
//! components and objects must use `uvm_component_param_utils or
//! `uvm_object_param_utils instead of non-parameterized utils macros.

use dv_model::Design;
use sv_syntax::SyntaxKind;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::uvm_classes::{At, Ix};

pub(crate) fn param_class_factory(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for (ti, c) in csts.iter().enumerate() {
        for n in c.nodes(ClassDecl) {
            if c.tree.child(n, Header).and_then(|h| c.tree.child(h, ParamList)).is_none() {
                continue;
            }
            let Some(ext) = crate::support::uvm_syntax::extends_clause(c, n) else { continue };
            let base = c.text(ext);
            if !base.contains("uvm_") {
                continue;
            }
            let comp = ["driver", "agent", "env", "component", "scoreboard"].iter().any(|k| base.contains(k));
            let Some(cls) = ix.class_of(At { tree: ti, node: n }) else { continue };
            for m in &cls.macros {
                let name = m.name.as_str();
                if !matches!(
                    name,
                    "uvm_component_utils" | "uvm_object_utils" | "uvm_component_utils_begin" | "uvm_object_utils_begin"
                ) {
                    continue;
                }
                let want = if comp { "`uvm_component_param_utils" } else { "`uvm_object_param_utils" };
                s.add(
                    "UVM-FAC-004",
                    "ParameterizedClassFactoryRegistration",
                    Severity::Error,
                    format!(
                        "Parameterized class '{}' declares standard non-parameterized macro '`{name}'. In UVM methodology (IEEE 1800.2 § 5.3), \
                         parameterized classes must use '{want}(...)' to ensure proper specialization and prevent \
                         factory type registry name collisions.",
                        cls.name
                    ),
                    m.loc,
                );
            }
        }
    }
}
