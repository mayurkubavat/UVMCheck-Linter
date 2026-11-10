//! Who raises and who drops objections in a class: the one answer the
//! objection rules share (UVM-OBJ-001, UVM-OBJ-003, UVM-PHS-006).
//!
//! An objection raised in one routine is often dropped in another one of
//! the same class: `pre_body`/`post_body`, `pre_start`/`post_start`, a
//! pair of helper tasks, a `phase_ready_to_end` callback. Looking at one
//! routine alone calls each half of such a pair a defect. [`Objections`]
//! counts the raises and drops of every routine of every class once, so a
//! rule can ask whether the other half is [`Objections::elsewhere`] in the
//! class, a base class or a derived class before it reports.

use std::collections::HashMap;

use dv_model::ClassId;
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

use crate::support::uvm_classes::{At, Ix};
use crate::support::uvm_syntax::{callee, routine_last_name, subtree};
use crate::syntax::Cst;

/// The two halves of an objection.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Half {
    Raise,
    Drop,
}

/// The `raise_objection` and `drop_objection` calls in a routine.
pub(crate) fn count(c: &Cst, routine: NodeId) -> (usize, usize) {
    let (mut raises, mut drops) = (0, 0);
    for n in subtree(c, routine) {
        if c.kind(n) == CallExpr {
            match callee(c, n) {
                "raise_objection" => raises += 1,
                "drop_objection" => drops += 1,
                _ => {}
            }
        }
    }
    (raises, drops)
}

/// Per class: its routines that raise or drop, with how often.
pub(crate) struct Objections {
    routines: HashMap<ClassId, Vec<(String, usize, usize)>>,
}

impl Objections {
    /// Counts every routine of every class of the compile, extern bodies
    /// included.
    pub(crate) fn new(ix: &Ix) -> Objections {
        let mut routines: HashMap<ClassId, Vec<(String, usize, usize)>> = HashMap::new();
        for cls in ix.d.classes() {
            for at in ix.all_routines(cls.id) {
                let c = ix.cst(at);
                let (raises, drops) = count(c, at.node);
                if raises + drops > 0 {
                    routines.entry(cls.id).or_default().push((routine_last_name(c, at.node), raises, drops));
                }
            }
        }
        Objections { routines }
    }

    /// Another routine than `routine`, in class `cls`, one of its base
    /// classes or a class derived from it, does more of `half` than of the
    /// other: it is where the
    /// objection `routine` leaves open is closed (or the one it closes is
    /// opened). A routine that raises and drops equally often is balanced
    /// in itself and is nobody's other half.
    pub(crate) fn elsewhere(&self, ix: &Ix, cls: ClassId, routine: &str, half: Half) -> bool {
        let has = |id: ClassId| {
            self.routines.get(&id).is_some_and(|v| {
                v.iter().any(|(name, raises, drops)| {
                    name != routine
                        && match half {
                            Half::Raise => raises > drops,
                            Half::Drop => drops > raises,
                        }
                })
            })
        };
        has(cls)
            || ix.d.ancestors(cls).any(|a| has(a.id))
            || ix.d.classes().iter().any(|k| has(k.id) && ix.d.ancestors(k.id).any(|a| a.id == cls))
    }

    /// [`Self::elsewhere`] for a routine node: false when it is in no class.
    pub(crate) fn elsewhere_for(&self, ix: &Ix, at: At, half: Half) -> bool {
        let Some(cls) = ix.routine_class(at) else { return false };
        self.elsewhere(ix, cls, &routine_last_name(ix.cst(at), at.node), half)
    }
}
