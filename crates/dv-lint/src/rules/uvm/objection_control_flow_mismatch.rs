//! UVM-OBJ-003 (ObjectionControlFlowMismatch): Performs control-flow graph
//! (CFG) analysis on UVM phase and task routines to verify that every execution
//! path containing raise_objection() is strictly guaranteed to reach a matching
//! drop_objection() without early returns or asymmetric conditional branching.
//!
//! Each objection handle of a routine (`phase`, `starting_phase`) is followed
//! on its own. A raise under a condition (`if (starting_phase != null)`) is
//! held under that condition, and a drop under the same condition balances
//! it. A routine whose other half is in another routine of the class
//! (`pre_body`/`post_body`, see [`crate::sem::objections`]) is not reported
//! for leaving the objection held, or for dropping one it did not raise.

use crate::sem::objections::{count, Half, Objections};
use crate::support::uvm_classes::{block_stmts, if_parts, is_component, is_sequence, receiver, Ix};
use crate::support::uvm_syntax::{body_items, callee, name_node, routine_last_name, stmt_expr, subtree};
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
    /// Held when a condition is true: the index of its text in `Flow::conds`.
    HeldIf(usize),
    Unknown,
}

/// What an expression statement does to an objection, and to which one:
/// the text of the handle it is called on (empty for a bare call).
fn objection_call(c: &Cst, stmt: NodeId) -> Option<(Half, String)> {
    let e = stmt_expr(c, stmt)?;
    for n in subtree(c, e) {
        if c.kind(n) != CallExpr {
            continue;
        }
        let half = match callee(c, n) {
            "raise_objection" => Half::Raise,
            "drop_objection" => Half::Drop,
            _ => continue,
        };
        let handle: String = receiver(c, n).map(|r| c.text(r)).unwrap_or_default().split_whitespace().collect();
        return Some((half, handle));
    }
    // Not a call the tree shows (inside a macro argument, say).
    let text = c.text(e);
    if text.contains("raise_objection") {
        Some((Half::Raise, String::new()))
    } else if text.contains("drop_objection") {
        Some((Half::Drop, String::new()))
    } else {
        None
    }
}

/// The objection handles a routine raises or drops, in source order.
fn handles(c: &Cst, items: &[NodeId]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for &i in items {
        for n in std::iter::once(i).chain(subtree(c, i)) {
            if c.kind(n) == ExprStmt {
                if let Some((_, h)) = objection_call(c, n) {
                    if !out.contains(&h) {
                        out.push(h);
                    }
                }
            }
        }
    }
    out
}

fn obj_str(o: Obj) -> &'static str {
    match o {
        Obj::Unheld => "Unheld",
        Obj::Held | Obj::HeldIf(_) => "Held",
        Obj::Unknown => "Unknown",
    }
}

struct Flow<'a, 'c> {
    c: &'a Cst<'c>,
    sub: String,
    /// The handle being followed.
    handle: String,
    /// The routine drops nothing itself and the class drops elsewhere: a
    /// `return` while held is the other routine's to close.
    closed_elsewhere: bool,
    /// Condition texts of `Obj::HeldIf`.
    conds: Vec<String>,
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
            ExprStmt => match objection_call(c, n).filter(|(_, h)| *h == self.handle) {
                Some((Half::Raise, _)) => {
                    if st == Obj::Held {
                        let msg = format!(
                            "Redundant 'raise_objection()' call in '{}': objection is already actively held along this control-flow path.",
                            self.sub
                        );
                        self.add(Severity::Warning, msg, n);
                    }
                    (false, Obj::Held)
                }
                Some((Half::Drop, _)) => {
                    if st == Obj::Unheld {
                        let msg = format!(
                            "Premature 'drop_objection()' in '{}': no active objection was raised along this execution path. \
                             Dropping unraised objections causes UVM runtime fatal errors.",
                            self.sub
                        );
                        self.add(Severity::Error, msg, n);
                    }
                    (false, Obj::Unheld)
                }
                None => (false, st),
            },
            TimedStmt => match c.items(n).into_iter().find(|&x| c.kind(x) != TimingControl) {
                Some(inner) => self.stmt(inner, st),
                None => (false, st),
            },
            // A loop's body is walked once from the state before the loop,
            // so a `return` while held or a drop of nothing inside it is
            // found. It may run any number of times: a body that changes
            // the state leaves it unknown after the loop.
            LoopStmt | DoWhileStmt => {
                let Some(body) = crate::support::procedural::loop_body(c, n) else { return (false, st) };
                let (_, after) = self.stmt(body, st);
                (false, if after == st { st } else { Obj::Unknown })
            }
            ReturnStmt => {
                if st == Obj::Held && !self.closed_elsewhere {
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
                let Some((cond, then, els)) = if_parts(c, n) else { return (false, st) };
                let cond: String = c.text(cond).split_whitespace().collect();
                // Held under this very condition: it is held in the 'if'
                // branch and not in the 'else' branch.
                let (st_then, st_else) = match st {
                    Obj::HeldIf(k) if self.conds[k] == cond => (Obj::Held, Obj::Unheld),
                    _ => (st, st),
                };
                let (tr, to) = self.stmt(then, st_then);
                let (fr, fo) = match els {
                    Some(e) => self.stmt(e, st_else),
                    None => (false, st_else),
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
                    // Raised under a condition and not otherwise: held
                    // under that condition (`if (phase != null) raise`).
                    if (to, fo) == (Obj::Held, Obj::Unheld) && st != Obj::Held {
                        let k = self.conds.iter().position(|x| *x == cond).unwrap_or_else(|| {
                            self.conds.push(cond);
                            self.conds.len() - 1
                        });
                        return (false, Obj::HeldIf(k));
                    }
                    // One side is not known: neither is the result.
                    if matches!(to, Obj::Unknown | Obj::HeldIf(_)) || matches!(fo, Obj::Unknown | Obj::HeldIf(_)) {
                        return (false, Obj::Unknown);
                    }
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
    let mut pairs: Option<Objections> = None;
    for cls in ix.model_classes() {
        if !is_component(d, cls.id) && !is_sequence(d, cls.id) {
            continue;
        }
        for at in ix.own_routines(cls.id) {
            let c = ix.cst(at);
            let sub = routine_last_name(c, at.node);
            let items = body_items(c, at.node);
            let (raises, drops) = count(c, at.node);
            if raises + drops == 0 {
                continue;
            }
            // The other half in another routine of the class or a base class.
            let pairs = pairs.get_or_insert_with(|| Objections::new(&ix));
            let closed_elsewhere = drops == 0 && pairs.elsewhere(&ix, cls.id, &sub, Half::Drop);
            let opened_elsewhere = raises == 0 && pairs.elsewhere(&ix, cls.id, &sub, Half::Raise);
            let start = if opened_elsewhere { Obj::Unknown } else { Obj::Unheld };
            for handle in handles(c, &items) {
                let mut flow =
                    Flow { c, sub: sub.clone(), handle, closed_elsewhere, conds: Vec::new(), out: Vec::new() };
                let (ret, st) = flow.list(&items, start);
                for (sev, msg, n) in std::mem::take(&mut flow.out) {
                    let Some(t) = c.first_tok(n) else { continue };
                    s.add("UVM-OBJ-003", "ObjectionControlFlowMismatch", sev, msg, c.loc(t));
                }
                if ret || closed_elsewhere || !matches!(st, Obj::Held | Obj::HeldIf(_)) {
                    continue;
                }
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
