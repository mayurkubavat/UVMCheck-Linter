//! RAL-BLK-001 (RegBlockNeverLocked): Flags a top-level register block (held by
//! an env or test, not by another block) on which lock_model() is never called.

use std::collections::HashSet;
use dv_model::{ClassId, Design};
use sv_syntax::SyntaxKind;
use crate::support::uvm_syntax::callee;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::uvm_classes::{At, Ix, derived, receiver};

pub(crate) fn reg_block_never_locked(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    let is_block = |id: ClassId| d.class(id).name != "uvm_reg_block" && derived(d, id, "uvm_reg_block");
    let mut blocks = Vec::new();
    let (mut top, mut sub, mut locked) = (HashSet::new(), HashSet::new(), HashSet::new());
    for cls in d.classes() {
        if is_block(cls.id) && !cls.is_parameterized {
            blocks.push(cls.id);
            if let Some(&at) = ix.decl.get(&cls.id) {
                if ix.cst(at).text(at.node).contains("lock_model") {
                    locked.insert(cls.id);
                }
            }
        }
        for p in &cls.properties {
            let Some(t) = p.type_class.filter(|&t| is_block(t)) else { continue };
            if is_block(cls.id) {
                sub.insert(t);
            } else {
                top.insert(t);
            }
        }
    }
    for (ti, c) in csts.iter().enumerate() {
        for n in c.nodes(CallExpr) {
            if callee(c, n) != "lock_model" {
                continue;
            }
            let Some(recv) = receiver(c, n) else { continue };
            let cx = ix.ctx(At { tree: ti, node: n });
            if let Some(k) = ix.expr_class(c, &cx, recv, 0).filter(|&k| is_block(k)) {
                locked.insert(k);
            }
        }
    }
    let related = |a: ClassId, b: ClassId| a == b || derived(d, a, &d.class(b).name) || derived(d, b, &d.class(a).name);
    for b in blocks {
        let t = top.iter().any(|&x| related(b, x));
        let u = sub.contains(&b);
        let l = locked.iter().any(|&x| related(b, x));
        if !t || u || l {
            continue;
        }
        let cls = d.class(b);
        s.add(
            "RAL-BLK-001",
            "RegBlockNeverLocked",
            Severity::Error,
            format!(
                "Register block '{}' is used as a top-level model but \
                 lock_model() is never called on it. Call lock_model() after build() (in the block's own \
                 build, or where the env creates it) so the address maps are finalized.",
                cls.name
            ),
            cls.loc,
        );
    }
}
