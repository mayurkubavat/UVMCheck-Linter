//! SV-COR-009 (CasexProhibited): Disallows use of 'casex' statements. 'casex'
//! treats unknown simulation 'x' values as wildcards, masking uninitialized
//! logic bugs and timing errors.

use crate::syntax::{case_statements, Cst};
use crate::{Severity, Sink};

pub(crate) fn casex_prohibited(c: &Cst, s: &mut Sink) {
    for (_, kw) in case_statements(c) {
        if c.text_of(kw) == "casex" {
            s.add(
                "SV-COR-009",
                "CasexProhibited",
                Severity::Error,
                "Use of 'casex' statement is prohibited in verification environments. \
                 In SystemVerilog (IEEE 1800 § 12.5.3), 'casex' treats both literal 'x'/'z' and unknown simulation 'x' bits as don't-care wildcards. \
                 If an uninitialized signal, timing race, or bug produces 'x', 'casex' silently matches the first matching branch, \
                 masking critical testbench and DUT failures. Replace 'casex' with 'casez' or 'case inside'."
                    .into(),
                c.loc(kw),
            );
        }
    }
}
