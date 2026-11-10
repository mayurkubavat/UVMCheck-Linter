//! UVM-VIF-DIR (VirtualInterfaceModportDriveInversion): Detects procedural
//! signal driving (via '=' or '<=') on virtual interface handles inside passive
//! uvm_monitor components.
//!
//! What the target is comes from its declarations ([`crate::sem::reach`]):
//! a signal reached through a virtual interface handle, whatever the
//! handle is called. A target that does not resolve is judged by its
//! names (`vif.`, `intf.`, `_vif.`), as the C++ engine judges all of them.

use crate::support::slang_ast::slang_trimmed;
use crate::support::uvm_classes::assign_parts;
use crate::syntax::Cst;
use crate::{derives_from, Severity, Sink};
use dv_model::Design;
use sv_sema::fx::FxMap as HashMap;

/// Whether the assignment on `line` of `file` to `lhs` reaches an interface
/// signal, where the declarations say.
fn reaches_interface(
    csts: &[Cst],
    targets: &HashMap<(u32, u32), Vec<(usize, sv_syntax::NodeId)>>,
    a: &dv_model::Assignment,
) -> Option<bool> {
    let here = targets.get(&(a.loc.file, a.loc.line))?;
    let mut answers = here
        .iter()
        .filter(|&&(i, l)| slang_trimmed(&csts[i], l) == a.lhs)
        .map(|&(i, l)| crate::sem::reach(&csts[i], l));
    let first = answers.next()??;
    Some(first.interface && (first.through_virtual || first.clocking))
}

pub(crate) fn virtual_interface_modport_drive_inversion(d: &Design, csts: &[Cst], out: &mut Sink) {
    // Assignment targets by file and line, to find the model's assignments in the trees.
    let mut targets: HashMap<(u32, u32), Vec<(usize, sv_syntax::NodeId)>> = HashMap::default();
    for (i, c) in csts.iter().enumerate() {
        for n in c.pre.iter().copied() {
            if let Some((lhs, _)) = assign_parts(c, n) {
                if let Some(t) = c.first_tok(lhs) {
                    let loc = c.loc(t);
                    targets.entry((loc.file, loc.line)).or_default().push((i, lhs));
                }
            }
        }
    }
    for cls in d.classes() {
        if cls.is_parameterized {
            continue;
        }
        if !cls.name.contains("monitor") && !derives_from(d, Some(cls.id), "uvm_monitor") {
            continue;
        }
        for sub in &cls.methods {
            if sub.is_extern {
                continue;
            }
            for a in &sub.assignments {
                let lhs = &a.lhs;
                let by_name = || lhs.contains("vif.") || lhs.contains("intf.") || lhs.contains("_vif.");
                if !reaches_interface(csts, &targets, a).unwrap_or_else(by_name) {
                    continue;
                }
                out.add(
                    "UVM-VIF-DIR",
                    "VirtualInterfaceModportDriveInversion",
                    Severity::Error,
                    format!(
                        "Passive uvm_monitor '{}' illegally drives virtual interface signal '{lhs}'. Monitors must be strictly read-only observers. Driving signals from a monitor \
violates UVM passive component semantics and causes multi-driver contention.",
                        cls.name
                    ),
                    a.loc,
                );
            }
        }
    }
}
