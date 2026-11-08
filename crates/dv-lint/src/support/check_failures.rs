//! Report severities and the failure branches of compare checks.



use sv_syntax::NodeId;

use crate::support::routine_bodies::{calls_in, if_parts, integer_literal, is_report_guard};



use crate::syntax::Cst;



use sv_syntax::SyntaxKind::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Level {
    None,
    Info,
    Warning,
    Error,
    Fatal,
}

/// `swe::strongestReport`.
pub(crate) fn strongest_report(c: &Cst, n: NodeId) -> Level {
    calls_in(c, n)
        .into_iter()
        .map(|k| match c.callee_name(k) {
            "$fatal" | "uvm_report_fatal" => Level::Fatal,
            "$error" | "uvm_report_error" => Level::Error,
            "$warning" | "uvm_report_warning" => Level::Warning,
            "$info" | "uvm_report_info" => Level::Info,
            _ => Level::None,
        })
        .max()
        .unwrap_or(Level::None)
}

pub(crate) fn has_compare_call(c: &Cst, e: NodeId) -> bool {
    calls_in(c, e).into_iter().any(|k| c.callee_name(k) == "compare")
}

pub(crate) fn is_zero(c: &Cst, e: NodeId) -> bool {
    let e = c.strip(e);
    if c.kind(e) != Literal {
        return false;
    }
    let t = c.expr_text(e);
    (t == "0" || t == "1'b0" || t == "'0") && (integer_literal(c, e).is_some() || t == "'0")
}

pub(crate) fn names_expected(t: &str) -> bool {
    t.contains("exp") || t.contains("ref") || t.contains("pred")
}

pub(crate) fn names_actual(t: &str) -> bool {
    ["act", "obs", "got", "rcv", "dut"].iter().any(|w| t.contains(w))
}

/// Which branch runs when the check fails: +1 then, -1 else, 0 not a check.
pub(crate) fn failing_branch(c: &Cst, cond: NodeId, what: &mut String) -> i32 {
    let cond = c.strip(cond);
    match c.kind(cond) {
        PrefixExpr => {
            let op = c.toks(cond).next().map(|t| c.text_of(t)).unwrap_or("");
            if op == "!" && c.tree.children(cond).next().is_some_and(|x| has_compare_call(c, x)) {
                *what = "compare()".into();
                return 1;
            }
            0
        }
        CallExpr => {
            if has_compare_call(c, cond) {
                *what = "compare()".into();
                return -1;
            }
            0
        }
        BinaryExpr => {
            let op = c.toks(cond).next().map(|t| c.text_of(t)).unwrap_or("");
            let eq = op == "==" || op == "===";
            let ne = op == "!=" || op == "!==";
            if !eq && !ne {
                return 0;
            }
            let mut sides = c.tree.children(cond);
            let (Some(l), Some(r)) = (sides.next(), sides.next()) else { return 0 };
            let (cl, cr) = (has_compare_call(c, l), has_compare_call(c, r));
            if (cl && is_zero(c, r)) || (cr && is_zero(c, l)) {
                *what = "compare()".into();
                return if eq { 1 } else { -1 };
            }
            let (lt, rt) = (c.expr_text(l).to_ascii_lowercase(), c.expr_text(r).to_ascii_lowercase());
            if (names_expected(&lt) && names_actual(&rt)) || (names_actual(&lt) && names_expected(&rt)) {
                *what = format!("'{}'", c.expr_text(cond));
                return if ne { 1 } else { -1 };
            }
            0
        }
        _ => 0,
    }
}

/// `meth::failureBranches`: (branch, condition, what).
pub(crate) fn failure_branches(c: &Cst, body: NodeId) -> Vec<(NodeId, NodeId, String)> {
    let mut v = Vec::new();
    for n in c.subtree(body) {
        if c.kind(n) != IfStmt || is_report_guard(c, n) {
            continue;
        }
        let (Some(cond), then, other) = if_parts(c, n) else { continue };
        let mut what = String::new();
        let side = failing_branch(c, cond, &mut what);
        let branch = match side {
            1 => then,
            -1 => other,
            _ => None,
        };
        if let Some(b) = branch {
            v.push((b, cond, what));
        }
    }
    v
}
