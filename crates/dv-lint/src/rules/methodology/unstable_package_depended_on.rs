//! SV-PKG-005 (UnstablePackageDependedOn): Computes instability I = Ce / (Ca +
//! Ce) for each user package from its imports and flags a package that depends
//! on a less stable one.

use crate::support::defines_packages::package_graph;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;

pub(crate) fn unstable_package_depended_on(d: &Design, csts: &[Cst], out: &mut Sink) {
    let g = package_graph(d, csts);
    for ((from, to), loc) in &g.edges {
        let (i_from, i_to) = (g.instability(from), g.instability(to));
        if i_from >= i_to {
            continue;
        }
        out.add(
            "SV-PKG-005",
            "UnstablePackageDependedOn",
            Severity::Info,
            format!(
                "Package '{from}' (instability {i_from:.2}, used by {} packages) depends on '{to}' (instability {i_to:.2}), which changes \
more easily. A change there ripples into everything that uses '{from}'. Invert the dependency or move the shared part into a \
stable package.",
                g.afferent.get(from).copied().unwrap_or(0)
            ),
            *loc,
        );
    }
}
