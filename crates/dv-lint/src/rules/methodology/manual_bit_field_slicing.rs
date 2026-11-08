//! SV-CNS-005 (ManualBitFieldSlicing): Flags a vector that is cut into three or
//! more different fixed part-selects (hdr[7:0], hdr[15:8], hdr[31:16]).

use crate::support::class_roles::{Index, Typedefs};
use crate::support::string_codes::{referenced, typedef_base, user_routines, Sym, SymInfo};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::{Design, Loc};
use std::collections::{BTreeSet, HashMap};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

/// An integral type that is not a struct or packed union.
fn is_integral_type(td: &Typedefs, ty: &str) -> bool {
    let t = typedef_base(td, ty);
    if t.contains('$') {
        return false;
    }
    const BASES: &[&str] = &["bit", "logic", "reg", "byte", "shortint", "int", "longint", "integer", "time", "enum"];
    BASES.iter().any(|b| {
        t.strip_prefix(b).is_some_and(|rest| !rest.starts_with(|ch: char| ch.is_ascii_alphanumeric() || ch == '_'))
    })
}

pub(crate) fn manual_bit_field_slicing(d: &Design, csts: &[Cst], out: &mut Sink) {
    const THRESHOLD: usize = 3;
    let ix = Index::new(csts);
    let mut uses: Vec<(SymInfo, BTreeSet<String>, Loc)> = Vec::new();
    let mut index: HashMap<Sym, usize> = HashMap::new();
    for r in user_routines(d, &ix) {
        let c = r.c;
        for n in c.subtree(r.body) {
            if c.kind(n) != SelectExpr || c.tok(n, ":").is_none() {
                continue;
            }
            let kids: Vec<NodeId> = c.tree.children(n).collect();
            let [v, left, right] = kids[..] else { continue };
            let vs = c.strip(v);
            if !matches!(c.kind(vs), NameExpr | MemberExpr) {
                continue;
            }
            let Some(s) = referenced(d, &ix, &r, vs) else { continue };
            if s.library || !is_integral_type(&ix.typedefs, &s.ty) {
                continue;
            }
            let i = *index.entry(s.sym.clone()).or_insert_with(|| {
                uses.push((s.clone(), BTreeSet::new(), c.loc_of(n)));
                uses.len() - 1
            });
            uses[i].1.insert(format!("{}:{}", c.expr_text(left), c.expr_text(right)));
        }
    }
    for (s, ranges, loc) in uses {
        if ranges.len() < THRESHOLD {
            continue;
        }
        let list: Vec<String> = ranges.iter().map(|r| format!("[{r}]")).collect();
        out.add(
            "SV-CNS-005",
            "ManualBitFieldSlicing",
            Severity::Info,
            format!(
                "'{}' is sliced into {} fixed fields ({}). The layout is spread over these numbers and breaks quietly when one changes. \
Declare a packed struct with named fields and use it (or cast to it) instead.",
                s.name,
                ranges.len(),
                list.join(", ")
            ),
            loc,
        );
    }
}
