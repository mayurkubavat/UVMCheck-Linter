//! PORT-PRP-001 (ProprietaryVendorCalls): Disallow unguarded
//! simulator-proprietary system tasks ($vcdpluson, $fsdbDumpvars, $shm_open).

use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

const VENDOR: &[&str] = &[
    "vcdpluson",
    "vcdplusoff",
    "vcdplusmemon",
    "vcdplusmemoff",
    "shm_open",
    "shm_close",
    "shm_probe",
    "fsdbDumpvars",
    "fsdbDumpfile",
    "fsdbDumpflush",
    "wlfdump",
    "wlfdumpvars",
];

pub(crate) fn vendor_calls(c: &Cst, s: &mut Sink) {
    for n in c.nodes(NameExpr) {
        let Some((a, b)) = c.tree.token_bounds(n) else { continue };
        let Some(name) = c.text_of(a).strip_prefix('$') else { continue };
        if a != b || !VENDOR.contains(&name) {
            continue;
        }
        s.add(
            "PORT-PRP-001",
            "ProprietaryVendorCalls",
            Severity::Warning,
            format!(
                "Proprietary simulator-specific system task '${name}' detected. \
                 Vendor-proprietary simulation tasks break VIP portability across simulators. \
                 Wrap proprietary calls in a simulator-specific `ifdef guard or use standard IEEE 1800 dump tasks ($dumpvars, $dumpfile)."
            ),
            c.loc(a),
        );
    }
}
