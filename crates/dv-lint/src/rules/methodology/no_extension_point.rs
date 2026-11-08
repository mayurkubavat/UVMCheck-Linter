//! UVM-PAT-003 (NoExtensionPoint): Flags a driver or monitor whose run_phase is
//! long (25 statements or more; option min_statements) while the class and its
//! user bases declare no virtual hook methods and use no callbacks.

use dv_model::Design;
use sv_syntax::NodeId;
use crate::support::routine_bodies::{if_parts, is_report_guard, is_stmt};
use crate::support::overrides::overrides_base;
use crate::support::class_roles::{class_methods, is_library, role, user_classes, Index, Role};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::SyntaxKind::*;

fn is_scaffolding(c: &Cst, s: NodeId) -> bool {
    match c.kind(s) {
        DataDecl => c
            .tree
            .children(s)
            .filter(|&x| c.kind(x) == Declarator)
            .filter_map(|x| c.tree.child(x, Name))
            .any(|x| c.tree.bare_text(x).starts_with("_local_report_object_")),
        ExprStmt => c
            .items(s)
            .first()
            .and_then(|&e| crate::support::free_routines::assignment_lhs(c, e))
            .is_some_and(|l| c.tree.bare_text(l).starts_with("_local_report_object_")),
        _ => false,
    }
}

/// `swe::countStatements`: statements, not blocks; a `uvm_* macro counts
/// as its report call.
pub(crate) fn count_statements(c: &Cst, n: NodeId) -> usize {
    let mut count = 0;
    count_into(c, n, &mut count);
    count
}

fn count_into(c: &Cst, n: NodeId, count: &mut usize) {
    if is_scaffolding(c, n) {
        return;
    }
    match c.kind(n) {
        IfStmt if is_report_guard(c, n) => {
            if let (_, Some(t), _) = if_parts(c, n) {
                count_into(c, t, count);
            }
            return;
        }
        DataDecl => {
            *count += c.tree.children(n).filter(|&x| c.kind(x) == Declarator).count().max(1);
            return;
        }
        SeqBlock | ParBlock | EmptyStmt | Body => {}
        _ if is_stmt(c, n) => *count += 1,
        _ => {}
    }
    let in_loop = c.kind(n) == LoopStmt;
    for k in c.node_kids(n) {
        match c.kind(k) {
            ElseClause => {
                for e in c.node_kids(k) {
                    count_into(c, e, count);
                }
            }
            DataDecl if !in_loop => count_into(c, k, count),
            _ if is_stmt(c, k) => count_into(c, k, count),
            _ => {}
        }
    }
}

pub(crate) fn no_extension_point(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    const MIN: usize = 25;
    for cls in user_classes(d) {
        let r = role(d, cls);
        if r != Role::Driver && r != Role::Monitor {
            continue;
        }
        let Some(run) = class_methods(cls).filter(|m| m.name == "run_phase").last() else { continue };
        let Some((c, body)) = ix.body(run) else { continue };
        let statements = count_statements(c, body);
        if statements < MIN {
            continue;
        }
        let mut hooks = false;
        for k in std::iter::once(cls).chain(d.ancestors(cls.id)) {
            if is_library(d, k) {
                break;
            }
            hooks |= class_methods(k).any(|m| m.is_virtual && !overrides_base(d, k, &m.name) && m.name != "new");
            if let Some((kc, kn)) = ix.decl(k.loc) {
                let text = kc.text(kn);
                hooks |= text.contains("uvm_callback") || text.contains("uvm_do_callbacks") || text.contains("uvm_register_cb");
            }
        }
        if hooks {
            continue;
        }
        out.add(
            "UVM-PAT-003",
            "NoExtensionPoint",
            Severity::Info,
            format!(
                "{} '{}' does all its work in a {statements}-statement run_phase and offers no virtual hook methods or callbacks. A test or a \
derived agent cannot change one step (error injection, timing, a protocol variant) without copying the whole run_phase. Split the steps into virtual methods \
(drive_item, wait_for_reset, collect_transfer) or add a uvm_callback.",
                if r == Role::Driver { "Driver" } else { "Monitor" },
                cls.name
            ),
            run.loc,
        );
    }
}
