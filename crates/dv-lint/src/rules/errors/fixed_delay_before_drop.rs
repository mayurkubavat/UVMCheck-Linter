//! UVM-TST-002 (FixedDelayBeforeDrop): Flags a constant #delay immediately
//! before drop_objection(), used to let traffic drain (xUnit 'sleepy test',
//! Sonar S2925).

use crate::support::ast::{call_site, kids, subtree, unparen, user_class_methods};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::HashSet;
use svlang::syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

/// `swe::exprText`: tokens joined, any trivia between two of them one space.
fn norm_text(c: &Cst, n: NodeId) -> String {
    let mut s = String::new();
    for t in c.tree.token_range(n) {
        let tok = &c.tree.tokens[t as usize];
        let spaced = if tok.from_macro { tok.space_before } else { !c.tree.trivia(t).is_empty() };
        if !s.is_empty() && spaced {
            s.push(' ');
        }
        s.push_str(&tok.text);
    }
    s
}

/// `swe::isConstantExpr` on syntax: literals, and operators on them (a
/// parameter name is constant too).
fn is_constant(c: &Cst, n: NodeId, params: &HashSet<String>) -> bool {
    let n = unparen(c, n);
    match c.kind(n) {
        Literal => true,
        NameExpr => c.first_tok(n).is_some_and(|t| params.contains(c.text_of(t))),
        BinaryExpr => kids(c, n).iter().all(|&k| is_constant(c, k, params)),
        PrefixExpr => kids(c, n).iter().all(|&k| is_constant(c, k, params)),
        _ => false,
    }
}

fn is_drop_stmt(c: &Cst, n: NodeId) -> bool {
    c.kind(n) == ExprStmt
        && kids(c, n).first().and_then(|&e| call_site(c, e)).is_some_and(|k| k.name == "drop_objection")
}

pub(crate) fn fixed_delay_before_drop(c: &Cst, s: &mut Sink) {
    let params: HashSet<String> = c
        .nodes(ParamDecl)
        .into_iter()
        .flat_map(|p| kids(c, p))
        .filter(|&d| c.kind(d) == Declarator)
        .filter_map(|d| c.tree.child(d, Name).map(|x| c.tree.bare_text(x)))
        .collect();
    let report = |t: NodeId, s: &mut Sink| {
        let Some(tc) = c.tree.child(t, TimingControl) else { return };
        let Some(e) = kids(c, tc).first().copied() else { return };
        let Some(at) = c.first_tok(t) else { return };
        s.add(
            "UVM-TST-002",
            "FixedDelayBeforeDrop",
            Severity::Warning,
            format!(
                "Fixed delay '{}' before drop_objection() guesses how long the testbench needs to drain. It is flaky \
                 when traffic grows and wastes time when it shrinks. Use the objection drain time \
                 (phase.phase_done.set_drain_time) or phase_ready_to_end() in the scoreboard.",
                norm_text(c, e)
            ),
            c.loc(at),
        );
    };
    let fixed = |t: NodeId| {
        c.tree.child(t, TimingControl).is_some_and(|tc| {
            c.tok(tc, "#").is_some() && kids(c, tc).first().is_some_and(|&e| is_constant(c, e, &params))
        })
    };
    for (_, _, methods) in user_class_methods(c) {
        for (m, _) in methods {
            let Some(body) = c.tree.child(m, Body) else { continue };
            for n in subtree(c, body) {
                if c.kind(n) == TimedStmt {
                    let inner = kids(c, n).get(1).copied();
                    if inner.is_some_and(|x| is_drop_stmt(c, x)) && fixed(n) {
                        report(n, s);
                    }
                }
                if matches!(c.kind(n), Body | SeqBlock | ParBlock) {
                    let items: Vec<NodeId> = c.items(n).into_iter().filter(|&x| c.kind(x) != DataDecl).collect();
                    for w in items.windows(2) {
                        let t = w[0];
                        if c.kind(t) != TimedStmt || !is_drop_stmt(c, w[1]) || !fixed(t) {
                            continue;
                        }
                        if kids(c, t).get(1).is_some_and(|&x| c.kind(x) == EmptyStmt) {
                            report(t, s);
                        }
                    }
                }
            }
        }
    }
}
