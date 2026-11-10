//! SV-CNC-003 (PollingLoop): Flags loops that test a flag or signal and wait a
//! constant #delay between tests, instead of wait(), @ or an event (busy-wait
//! smell, Sonar S2925).

use crate::support::procedural::{
    add_n, if_parts, is_task_call, is_wait_loop, loop_body, loop_kw, name_sym, routine_name, start, timing_kind,
    timing_of, written, Sym,
};
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::HashSet;
use svlang::syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

/// A `while` or `do ... while` condition.
pub(crate) fn loop_cond(c: &Cst, l: NodeId) -> Option<NodeId> {
    let k: Vec<NodeId> = kids(c, l).into_iter().filter(|&x| !matches!(c.kind(x), StmtLabel | Attribute)).collect();
    if c.kind(l) == DoWhileStmt {
        k.get(1).copied()
    } else {
        k.first().copied()
    }
}

/// `swe::isConstantExpr`: literals, parameters and operators over them.
pub(crate) fn is_constant(c: &Cst, e: NodeId) -> bool {
    match c.kind(e) {
        Literal => true,
        ParenExpr => kids(c, e).first().is_some_and(|&x| is_constant(c, x)),
        NameExpr => {
            let Some(t) = c.first_tok(e) else { return false };
            lookup(c, e, c.text_of(t)).is_some_and(|d| d.kind == DeclKind::Parameter)
        }
        BinaryExpr => {
            let k = kids(c, e);
            k.len() == 2 && is_constant(c, k[0]) && is_constant(c, k[1])
        }
        _ => false,
    }
}

/// A delay control (`#d`) with a constant delay.
fn is_fixed_delay(c: &Cst, t: NodeId) -> bool {
    timing_kind(c, t) == "#" && kids(c, t).first().is_some_and(|&x| is_constant(c, x))
}

/// `swe::referencedSymbols`: names and members an expression reads.
fn referenced(c: &Cst, e: NodeId) -> HashSet<Sym> {
    let mut out = HashSet::new();
    for x in subtree(c, e) {
        match c.kind(x) {
            NameExpr => {
                if let Some((s, _, _)) = name_sym(c, x) {
                    out.insert(s);
                }
            }
            MemberExpr => {
                let is_callee = c.parent(x).is_some_and(|p| c.kind(p) == CallExpr && kids(c, p).first() == Some(&x));
                if !is_callee {
                    out.insert(Sym::Member(c.last_text(x).to_string()));
                }
            }
            _ => {}
        }
    }
    out
}

/// The statement (with nested ones) contains a `break` or `return`.
fn exits(c: &Cst, st: NodeId) -> bool {
    subtree(c, st).into_iter().any(|x| match c.kind(x) {
        ReturnStmt => true,
        JumpStmt => c.keyword(x).is_some_and(|k| c.text_of(k) == "break"),
        _ => false,
    })
}

pub(crate) fn polling_loop(c: &Cst, s: &mut Sink) {
    for (r, n) in user_routines(c) {
        let where_ = format!("'{}'", routine_name(c, r));
        for l in subtree(c, r) {
            if !is_wait_loop(c, l) {
                continue;
            }
            let Some(body) = loop_body(c, l) else { continue };
            let (mut fixed, mut real) = (false, false);
            let mut exit_conds = Vec::new();
            for x in subtree(c, body) {
                match c.kind(x) {
                    TimedStmt => {
                        if let Some(t) = timing_of(c, x) {
                            if timing_kind(c, t) == "#" {
                                if is_fixed_delay(c, t) {
                                    fixed = true;
                                } else {
                                    real = true;
                                }
                            } else {
                                real = true;
                            }
                        }
                    }
                    WaitStmt => {
                        if c.keyword(x).is_some_and(|k| c.text_of(k) == "wait_order")
                            || !c.toks(x).any(|t| c.text_of(t) == "fork")
                        {
                            real = true;
                        }
                    }
                    CallExpr => {
                        if is_task_call(c, x) {
                            real = true;
                        }
                    }
                    IfStmt => {
                        let (cond, then, _) = if_parts(c, x);
                        if then.is_some_and(|t| exits(c, t)) {
                            if let Some(cd) = cond {
                                exit_conds.push(cd);
                            }
                        }
                    }
                    _ => {}
                }
            }
            if !fixed || real {
                continue;
            }
            let conds = if c.kind(l) == DoWhileStmt || loop_kw(c, l) == "while" {
                let Some(cd) = loop_cond(c, l) else { continue };
                if is_constant(c, cd) {
                    continue;
                }
                vec![cd]
            } else {
                exit_conds
            };
            if conds.is_empty() {
                continue;
            }
            let w: HashSet<Sym> = written(c, body, true).into_iter().map(|x| x.0).collect();
            for cd in conds {
                if referenced(c, cd).iter().any(|x| w.contains(x)) {
                    continue;
                }
                let t = norm_text(c, cd);
                if let Some(loc) = start(c, l) {
                    add_n(
                        s,
                        n,
                        "SV-CNC-003",
                        "PollingLoop",
                        Severity::Warning,
                        format!(
                            "Loop in {where_} polls '{t}' with a fixed delay. Polling ties the check to the timescale, misses short pulses and slows simulation; wait for the change instead with wait({t}), @(...) on the signal or clocking block, or an event."
                        ),
                        loc,
                    );
                }
                break;
            }
        }
    }
}
