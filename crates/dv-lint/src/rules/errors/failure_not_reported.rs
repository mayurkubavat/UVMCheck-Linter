//! SV-ERR-001 (FailureNotReported): Flags randomize(), $cast(),
//! uvm_config_db::get(), try_* and compare() status checks whose failure branch
//! is empty or only reports below error severity (Sonar S108/S1166).

use crate::support::class_index::{is_report_guard, is_report_scaffolding, report_level, Level};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

#[derive(Clone, Copy, PartialEq)]
enum Status {
    Randomize,
    Cast,
    ConfigGet,
    TryOp,
    Compare,
}

fn status_name(s: Status) -> &'static str {
    match s {
        Status::Randomize => "randomize()",
        Status::Cast => "$cast()",
        Status::ConfigGet => "uvm_config_db::get()",
        Status::TryOp => "try_* call",
        Status::Compare => "compare()",
    }
}

fn classify(c: &Cst, e: NodeId) -> Option<Status> {
    let e = crate::support::statements::strip_parens(c, e);
    let n = crate::support::statements::call_name(c, e)?;
    match n {
        "randomize" => Some(Status::Randomize),
        "$cast" => Some(Status::Cast),
        "try_get" | "try_put" | "try_peek" => Some(Status::TryOp),
        "compare" => Some(Status::Compare),
        "get" if c.expr_text(e).contains("uvm_config_db") => Some(Status::ConfigGet),
        _ => None,
    }
}

/// The status call a condition tests for failure (`!call`, `call == 0`).
fn failure_test(c: &Cst, cond: NodeId) -> Option<Status> {
    let e = crate::support::statements::strip_parens(c, cond);
    let kids: Vec<NodeId> = c.tree.children(e).collect();
    let op = c.toks(e).next().map(|t| c.text_of(t)).unwrap_or("");
    match c.kind(e) {
        PrefixExpr if op == "!" => classify(c, *kids.first()?),
        BinaryExpr if op == "==" || op == "===" => {
            let r = c.expr_text(*kids.get(1)?);
            if matches!(r.as_str(), "0" | "1'b0" | "'0") {
                classify(c, *kids.first()?)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// The statement does nothing but report below error severity (or nothing).
fn only_weak_reports(c: &Cst, st: NodeId, strongest: &mut Level) -> bool {
    match c.kind(st) {
        EmptyStmt => true,
        SeqBlock => c.items(st).into_iter().all(|x| only_weak_reports(c, x, strongest)),
        IfStmt => {
            // `uvm_* macros expand to a report guard around uvm_report_*(...).
            let (_, then, other) = crate::support::routine_bodies::if_parts(c, st);
            if other.is_some() || c.tree.child(st, ElseClause).is_some() || !is_report_guard(c, st) {
                return false;
            }
            then.is_none_or(|t| only_weak_reports(c, t, strongest))
        }
        DataDecl => is_report_scaffolding(c, st),
        ExprStmt => {
            if is_report_scaffolding(c, st) {
                return true;
            }
            let Some(e) = c.tree.children(st).next().map(|e| crate::support::statements::strip_parens(c, e)) else {
                return false;
            };
            if c.kind(e) != CallExpr {
                return false;
            }
            let level = report_level(c, e);
            let n = crate::support::statements::call_name(c, e).unwrap_or("");
            if level == Level::None && n != "$display" && n != "$write" {
                return false;
            }
            if level >= Level::Error {
                return false;
            }
            if level > *strongest {
                *strongest = level;
            }
            true
        }
        _ => false,
    }
}

pub(crate) fn failure_not_reported(c: &Cst, out: &mut Sink) {
    for st in c.nodes_of(&[IfStmt, AssertStmt]) {
        let times = crate::support::sva_cov::user_code_times(c, st);
        if times == 0 {
            continue;
        }
        let found = if c.kind(st) == IfStmt {
            let (Some(cond), then, other) = crate::support::routine_bodies::if_parts(c, st) else { continue };
            if c.kind(cond) == MatchExpr {
                continue;
            }
            match failure_test(c, cond) {
                Some(s) => then.map(|t| (s, t)),
                None => classify(c, cond).zip(other),
            }
        } else {
            // `assert (call) else ...`
            if c.toks(st).next().is_none_or(|t| c.text_of(t) != "assert") {
                continue;
            }
            let cond = c.tree.children(st).next();
            let other = c.tree.child(st, ElseClause).and_then(|e| c.items(e).into_iter().next());
            cond.and_then(|x| classify(c, x)).zip(other)
        };
        let Some((s, branch)) = found else { continue };
        let cond = c.tree.children(st).next();
        if cond.is_some_and(|x| !crate::support::sema_facts::ast_ok(c, x)) {
            continue;
        }
        let mut strongest = Level::None;
        if !only_weak_reports(c, branch, &mut strongest) {
            continue;
        }
        // A missing optional config_db entry reported as a warning is a common, valid choice.
        if s == Status::ConfigGet && strongest == Level::Warning {
            continue;
        }
        let how = match strongest {
            Level::None => "is silently ignored",
            Level::Info => "is only reported with `uvm_info",
            _ => "is only reported as a warning",
        };
        for _ in 0..times {
            out.add(
                "SV-ERR-001",
                "FailureNotReported",
                Severity::Error,
                format!(
                    "Failure of {} {how}, so the test still passes after detecting it. Report it with `uvm_error or `uvm_fatal, or handle the \
failure explicitly.",
                    status_name(s)
                ),
                c.loc_of(st),
            );
        }
    }
}
