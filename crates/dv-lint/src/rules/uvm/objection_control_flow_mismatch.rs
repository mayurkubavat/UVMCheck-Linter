//! UVM-OBJ-003 (ObjectionControlFlowMismatch): Performs control-flow graph
//! (CFG) analysis on UVM phase and task routines to verify that every execution
//! path containing raise_objection() is strictly guaranteed to reach a matching
//! drop_objection() without early returns or asymmetric conditional branching.

use crate::support::uvm_classes::{block_stmts, if_parts, is_component, is_sequence, Ix};
use crate::support::uvm_syntax::{body_items, callee, name_node, routine_last_name, stmt_expr};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use sv_syntax::{Child, NodeId, SyntaxKind};
use SyntaxKind::*;

/// A case statement's items: each statement, and whether it is the default.
fn case_items(c: &Cst, n: NodeId) -> Vec<(NodeId, bool)> {
    let mut out = Vec::new();
    let mut depth = 0;
    let mut seen_selector = false;
    let mut expect = false;
    let mut default = false;
    for k in c.kids(n) {
        match *k {
            Child::Token(t) => match c.text_of(t) {
                "(" => depth += 1,
                ")" => {
                    depth -= 1;
                    if depth == 0 {
                        seen_selector = true;
                    }
                }
                ":" if seen_selector && depth == 0 => expect = true,
                "default" if seen_selector => {
                    expect = true;
                    default = true;
                }
                _ => {}
            },
            Child::Node(x) => {
                if matches!(c.kind(x), StmtLabel | Attribute) {
                    continue;
                }
                if expect {
                    out.push((x, default));
                    expect = false;
                    default = false;
                }
            }
        }
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Obj {
    Unheld,
    Held,
    Unknown,
}

fn obj_str(o: Obj) -> &'static str {
    match o {
        Obj::Unheld => "Unheld",
        Obj::Held => "Held",
        Obj::Unknown => "Unknown",
    }
}

struct Flow<'a, 'c> {
    c: &'a Cst<'c>,
    sub: String,
    out: Vec<(Severity, String, NodeId)>,
}

impl Flow<'_, '_> {
    fn add(&mut self, sev: Severity, msg: String, at: NodeId) {
        self.out.push((sev, msg, at));
    }

    /// `evaluateStatement`: returns whether the statement always returns,
    /// with the objection state after it.
    fn stmt(&mut self, n: NodeId, st: Obj) -> (bool, Obj) {
        let c = self.c;
        match c.kind(n) {
            SeqBlock | ParBlock | Body => self.list(&block_stmts(c, n), st),
            ExprStmt => {
                let text = stmt_expr(c, n).map(|e| c.text(e)).unwrap_or_default();
                let named =
                    stmt_expr(c, n).map(|e| if c.kind(e) == CallExpr { callee(c, e) } else { "" }).unwrap_or("");
                if named == "raise_objection" || text.contains("raise_objection") {
                    if st == Obj::Held {
                        let msg = format!(
                            "Redundant 'raise_objection()' call in '{}': objection is already actively held along this control-flow path.",
                            self.sub
                        );
                        self.add(Severity::Warning, msg, n);
                    }
                    (false, Obj::Held)
                } else if named == "drop_objection" || text.contains("drop_objection") {
                    if st == Obj::Unheld {
                        let msg = format!(
                            "Premature 'drop_objection()' in '{}': no active objection was raised along this execution path. \
                             Dropping unraised objections causes UVM runtime fatal errors.",
                            self.sub
                        );
                        self.add(Severity::Error, msg, n);
                    }
                    (false, Obj::Unheld)
                } else {
                    (false, st)
                }
            }
            TimedStmt => match c.items(n).into_iter().find(|&x| c.kind(x) != TimingControl) {
                Some(inner) => self.stmt(inner, st),
                None => (false, st),
            },
            ReturnStmt => {
                if st == Obj::Held {
                    let msg = format!(
                        "Early 'return' in '{}' while phase objection is still held. \
                         Bypassing drop_objection() causes simulation to hang indefinitely.",
                        self.sub
                    );
                    self.add(Severity::Error, msg, n);
                }
                (true, st)
            }
            IfStmt => {
                let Some((_, then, els)) = if_parts(c, n) else { return (false, st) };
                let (tr, to) = self.stmt(then, st);
                let (fr, fo) = match els {
                    Some(e) => self.stmt(e, st),
                    None => (false, st),
                };
                if tr && fr {
                    return (true, to);
                }
                if tr {
                    return (false, fo);
                }
                if fr {
                    return (false, to);
                }
                if to != fo {
                    let msg = format!(
                        "Asymmetric objection state across conditional branch in '{}': 'if' branch leaves objection as {}, while 'else' branch leaves objection as {}. \
                         Unbalanced branch objections lead to unpredictable simulation termination or runtime fatals.",
                        self.sub,
                        obj_str(to),
                        obj_str(fo)
                    );
                    self.add(Severity::Error, msg, n);
                    return (false, Obj::Unknown);
                }
                (false, to)
            }
            CaseStmt if crate::support::uvm_syntax::case_keyword(c, n).is_some() => {
                let items = case_items(c, n);
                let mut outs = Vec::new();
                let mut all_return = true;
                let mut has_default = false;
                // slang keeps the default apart and checks it after the items.
                let (defaults, normal): (Vec<_>, Vec<_>) = items.into_iter().partition(|x| x.1);
                for (i, _) in normal.into_iter().chain(defaults) {
                    let (r, o) = self.stmt(i, st);
                    if !r {
                        all_return = false;
                    }
                    outs.push(o);
                    has_default |= false;
                }
                if !c.toks(n).any(|t| c.text_of(t) == "default") {
                    all_return = false;
                    outs.push(st);
                }
                let _ = has_default;
                if all_return {
                    return (true, outs.first().copied().unwrap_or(st));
                }
                if outs.iter().skip(1).any(|&o| o != outs[0]) {
                    let msg = format!(
                        "Asymmetric objection state in case statement inside '{}'. Different case choices result in divergent objection states.",
                        self.sub
                    );
                    self.add(Severity::Error, msg, n);
                    return (false, Obj::Unknown);
                }
                (false, outs.first().copied().unwrap_or(st))
            }
            _ => (false, st),
        }
    }

    fn list(&mut self, items: &[NodeId], st: Obj) -> (bool, Obj) {
        let mut cur = st;
        for &i in items {
            let (r, next) = self.stmt(i, cur);
            if r {
                return (true, next);
            }
            cur = next;
        }
        (false, cur)
    }
}

pub(crate) fn objection_control_flow(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for cls in ix.model_classes() {
        if !is_component(d, cls.id) && !is_sequence(d, cls.id) {
            continue;
        }
        for at in ix.own_routines(cls.id) {
            let c = ix.cst(at);
            let sub = routine_last_name(c, at.node);
            let mut flow = Flow { c, sub: sub.clone(), out: Vec::new() };
            let (ret, st) = flow.list(&body_items(c, at.node), Obj::Unheld);
            for (sev, msg, n) in std::mem::take(&mut flow.out) {
                let Some(t) = c.first_tok(n) else { continue };
                s.add("UVM-OBJ-003", "ObjectionControlFlowMismatch", sev, msg, c.loc(t));
            }
            if !ret && st == Obj::Held {
                let Some(loc) =
                    name_node(c, at.node).and_then(|x| c.tree.token_range(x).last().copied()).map(|t| c.loc(t))
                else {
                    continue;
                };
                s.add(
                    "UVM-OBJ-003",
                    "ObjectionControlFlowMismatch",
                    Severity::Error,
                    format!(
                        "Subroutine '{sub}' in class '{}' reaches routine exit while a phase objection remains held. \
                         Every execution path must call 'phase.drop_objection()' before completion to prevent simulation hangs.",
                        cls.name
                    ),
                    loc,
                );
            }
        }
    }
}
