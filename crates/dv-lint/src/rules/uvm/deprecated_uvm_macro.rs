//! UVM-API-002 (DeprecatedUvmMacro): Flags uses of UVM macros that the UVM
//! version being compiled defines only under UVM_ENABLE_DEPRECATED_API (in UVM
//! 2020: `uvm_do_with, `uvm_do_on, `uvm_do_pri, `uvm_create_on, `uvm_send_pri
//! and the rest of that family).

use std::collections::HashSet;
use dv_model::Design;
use sv_lexer::Kind;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::defines_packages::{Files, api_pack, live, origin_loc};

pub(crate) fn deprecated_uvm_macro(_: &Design, csts: &[Cst], out: &mut Sink) {
    let Some(pack) = api_pack(csts) else { return };
    let mut seen = HashSet::new();
    for c in csts {
        let fs = Files::new(c);
        for (f, lx) in fs.lexed.iter().enumerate() {
            if fs.lib[f] {
                continue;
            }
            let mut i = 0;
            while i < lx.toks.len() {
                if lx.toks[i].0 != Kind::Directive || !live(c.tree, f as u32, lx.toks[i].1) {
                    i += 1;
                    continue;
                }
                let text = lx.text(i);
                if text == "`define" {
                    // A use in a macro body is the expansion's.
                    i = lx.rest_of_line(i).end;
                    continue;
                }
                let name = &text[1..];
                if pack.deprecated_macros.contains(name) && seen.insert((fs.path(f as u32), lx.toks[i].1)) {
                    out.add(
                        "UVM-API-002",
                        "DeprecatedUvmMacro",
                        Severity::Warning,
                        format!(
                            "`{name} is deprecated in UVM {}: the library defines it only when UVM_ENABLE_DEPRECATED_API is set, so this line \
stops compiling without that define. For the `uvm_do family use `uvm_do(SEQ_OR_ITEM, SEQR, PRIORITY, {{CONSTRAINTS}}) and its \
`uvm_send / `uvm_rand_send forms with the extra arguments.",
                            pack.version
                        ),
                        origin_loc(c, f as u32, lx.toks[i].1),
                    );
                }
                i += 1;
            }
        }
    }
}
