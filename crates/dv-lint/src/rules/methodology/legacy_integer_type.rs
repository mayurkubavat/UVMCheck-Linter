//! SV-CNS-007 (LegacyIntegerType): Flags the Verilog-1995 type 'integer' in
//! class code.

use dv_model::Design;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::defines_packages::{Container, Files, container_of, user_types};

pub(crate) fn legacy_integer_type(_: &Design, csts: &[Cst], out: &mut Sink) {
    for c in csts {
        let fs = Files::new(c);
        for (n, kw) in user_types(c, &fs, "integer") {
            if container_of(c, n) != Container::Class {
                continue;
            }
            out.add(
                "SV-CNS-007",
                "LegacyIntegerType",
                Severity::Info,
                "'integer' in class code is a 4-state, signed 32-bit legacy type. Use int for counters and indices, or logic [N-1:0] \
when X and Z must be kept."
                    .into(),
                c.loc(kw),
            );
        }
    }
}
