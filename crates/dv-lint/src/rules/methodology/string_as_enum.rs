//! SV-CNS-004 (StringAsEnum): Flags a string variable compared against three or
//! more literals in one case statement or if/else-if chain.

use crate::support::class_roles::{is_library, Index};
use crate::support::routine_bodies::Calls;
use crate::support::string_codes::{
    all_uvm_access_policies, dispatches, literal_list, op_of, referenced, user_routines, Routine, Sym,
};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use std::collections::HashSet;
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

/// Variables assigned the result of a UVM library call (s = f.get_access()).
fn from_library(d: &Design, ix: &Index, r: &Routine) -> HashSet<Sym> {
    let c = r.c;
    let mut out = HashSet::new();
    let Some(m) = r.m else { return out };
    let calls = Calls::new(m);
    for n in c.subtree(r.body) {
        if c.kind(n) != BinaryExpr || op_of(c, n) != "=" {
            continue;
        }
        let kids: Vec<NodeId> = c.tree.children(n).collect();
        let [lhs, rhs] = kids[..] else { continue };
        let rhs = c.strip(rhs);
        if c.kind(rhs) != CallExpr || c.callee_name(rhs).starts_with('$') {
            continue;
        }
        let owner = calls.owner(c, rhs);
        let lib = !owner.is_empty()
            && (owner.starts_with("uvm_") || d.classes().iter().any(|k| k.name == owner && is_library(d, k)));
        if !lib {
            continue;
        }
        if let Some(s) = referenced(d, ix, r, lhs) {
            out.insert(s.sym);
        }
    }
    out
}

pub(crate) fn string_as_enum(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for r in user_routines(d, &ix) {
        let lib = from_library(d, &ix, &r);
        for x in dispatches(d, &ix, &r, false, crate::options::get("SV-CNS-004", "min_values", 3) as _) {
            if lib.contains(&x.sym.sym) || all_uvm_access_policies(&x.lits) {
                continue;
            }
            out.add(
                "SV-CNS-004",
                "StringAsEnum",
                Severity::Warning,
                format!(
                    "String '{}' is compared against {} literals ({}) to choose what to do. The compiler cannot catch a misspelt or missing \
value. Declare an enum and convert the string once where it enters, for example with uvm_enum_wrapper#(T)::from_name().",
                    x.sym.name,
                    x.lits.len(),
                    literal_list(&x.lits)
                ),
                x.loc,
            );
        }
    }
}
