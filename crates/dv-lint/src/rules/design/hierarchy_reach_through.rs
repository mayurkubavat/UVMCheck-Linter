//! UVM-ARC-002 (HierarchyReachThrough): Flags expressions that reach through
//! three or more component handles (env.agt.drv.vif), outside connect_phase
//! wiring (PMD LawOfDemeter, Fowler 'message chains').

use crate::support::class_index::*;
use crate::support::class_metrics::{globals, is_method_ref, Env, Ty};
use crate::support::statements::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::HashSet;
use svlang::syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn hierarchy_reach_through(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    let g = globals(c);
    for i in classes.user() {
        let cls = &classes.list[i];
        for m in &cls.methods {
            if m.name == "connect_phase" || m.name == "end_of_elaboration_phase" {
                continue;
            }
            let Some(body) = m.body else { continue };
            let env = Env::new(c, &classes, Some(i), Some(m.decl), &g);
            let is_component = |t: &Option<Ty>| {
                t.as_ref().is_some_and(|t| !t.array && t.class.is_some_and(|k| classes.role(k).is_component()))
            };
            let mut inner: HashSet<u32> = HashSet::new();
            for top in subtree(c, body) {
                if c.kind(top) != MemberExpr || inner.contains(&top.0) || is_method_ref(c, &env, top) {
                    continue;
                }
                let mut hops = 0;
                let Some(mut e) = c.tree.children(top).next() else {
                    continue;
                };
                loop {
                    let mut x = strip_parens(c, e);
                    while c.kind(x) == SelectExpr {
                        match c.tree.children(x).next() {
                            Some(v) => x = strip_parens(c, v),
                            None => break,
                        }
                    }
                    if is_component(&env.expr_type(e)) {
                        let this = c.kind(x) == NameExpr && c.toks(x).next().is_some_and(|t| c.text_of(t) == "this");
                        if !this {
                            hops += 1;
                        }
                    }
                    if c.kind(x) != MemberExpr {
                        break;
                    }
                    inner.insert(x.0);
                    match c.tree.children(x).next() {
                        Some(v) => e = v,
                        None => break,
                    }
                }
                if hops < 3 {
                    continue;
                }
                let tt = env.expr_type(top);
                let sequencer = tt.is_some_and(|t| {
                    !t.array
                        && (t.class.is_some_and(|k| classes.role(k) == Role::Sequencer)
                            || t.name.starts_with("uvm_sequencer"))
                });
                if sequencer {
                    continue;
                }
                s.add(
                    "UVM-ARC-002",
                    "HierarchyReachThrough",
                    Severity::Warning,
                    format!(
                        "'{}' reaches through {hops} component handles from {}::{}. It breaks as soon as the env is restructured and couples this code to other \
                         components' internals (Law of Demeter). Pass what is needed through config_db, a \
                         config object or a TLM connection.",
                        normalized(c, top),
                        cls.name,
                        m.name
                    ),
                    start(c, top),
                );
            }
        }
    }
}
