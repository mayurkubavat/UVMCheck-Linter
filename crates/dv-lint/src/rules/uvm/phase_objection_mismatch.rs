//! UVM-PHS-006 (PhaseObjectionMismatch): Detects 'phase.raise_objection()'
//! calls without a matching 'phase.drop_objection()' in the same task or
//! method.

use crate::sem::objections::{Half, Objections};
use crate::support::uvm_classes::{At, Ix};
use crate::support::uvm_syntax::{body_items, callee, routine_last_name, routines, subtree};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use sv_syntax::SyntaxKind::*;

pub(crate) fn phase_objection_mismatch(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    let mut pairs: Option<Objections> = None;
    for (tree, c) in csts.iter().enumerate() {
        check(&ix, &mut pairs, tree, c, s);
    }
}

fn check(ix: &Ix, pairs: &mut Option<Objections>, tree: usize, c: &Cst, s: &mut Sink) {
    for f in routines(c) {
        let (mut raise, mut drop, mut first) = (0usize, 0usize, None);
        for item in body_items(c, f) {
            for n in subtree(c, item) {
                if c.kind(n) != CallExpr {
                    continue;
                }
                match callee(c, n) {
                    "raise_objection" => {
                        raise += 1;
                        if first.is_none() {
                            first = c.first_tok(n);
                        }
                    }
                    "drop_objection" => drop += 1,
                    _ => {}
                }
            }
        }
        if raise > drop {
            let Some(at) = first else { continue };
            // Dropped in another routine of the class (pre_body/post_body).
            if pairs.get_or_insert_with(|| Objections::new(ix)).elsewhere_for(ix, At { tree, node: f }, Half::Drop) {
                continue;
            }
            s.add(
                "UVM-PHS-006",
                "PhaseObjectionMismatch",
                Severity::Error,
                format!(
                    "UVM phase objection raised via 'raise_objection()' in '{}()' without matching 'drop_objection()'. Unpaired phase objections prevent phase completion and cause simulation to hang indefinitely.",
                    routine_last_name(c, f)
                ),
                c.loc(at),
            );
        }
    }
}
