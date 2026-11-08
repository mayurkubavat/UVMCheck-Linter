//! SV-PKG-004 (PackageLayerInversion): Flags a package that imports a package
//! from a higher testbench layer, such as an agent package importing an env or
//! test package.

use dv_model::Design;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::defines_packages::{package_graph};

fn layer_name(l: i32) -> &'static str {
    match l {
        1 => "agent",
        2 => "env",
        3 => "test",
        _ => "object",
    }
}

pub(crate) fn package_layer_inversion(d: &Design, csts: &[Cst], out: &mut Sink) {
    let g = package_graph(d, csts);
    for ((from, to), loc) in &g.edges {
        let lf = g.layers.get(from).copied().unwrap_or(0);
        let lt = g.layers.get(to).copied().unwrap_or(0);
        if lf == 0 || lt <= lf {
            continue;
        }
        let (nf, nt) = (layer_name(lf), layer_name(lt));
        out.add(
            "SV-PKG-004",
            "PackageLayerInversion",
            Severity::Warning,
            format!(
                "{nf} package '{from}' imports {nt} package '{to}'. Dependencies must point down the stack (item, sequence, agent, env, \
test); otherwise the {nf} cannot be reused without the {nt}. Move what is shared into the lower package or a common one."
            ),
            *loc,
        );
    }
}
