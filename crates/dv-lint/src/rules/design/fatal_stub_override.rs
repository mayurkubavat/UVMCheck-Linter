//! SV-DSN-013 (FatalStubOverride): Flags an override of a user base-class
//! method whose whole body is an error or fatal report ("not supported here").

use crate::support::class_index::{
    is_report_guard, is_report_scaffolding, nested_stmts, report_level, strongest_report, Classes, Level,
};
use crate::support::statements::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

pub(crate) fn fatal_stub_override(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    for i in classes.user() {
        let cls = &classes.list[i];
        for m in &cls.methods {
            let Some((k, Ok(bi))) = classes.override_of(i, &m.name) else {
                continue;
            };
            if classes.list[k].library {
                continue;
            }
            let Some(body) = m.body else { continue };
            let level = strongest_report(c, body);
            if level != Level::Error && level != Level::Fatal {
                continue;
            }
            if !only_reports(c, body) {
                continue;
            }
            s.add(
                "SV-DSN-013",
                "FatalStubOverride",
                Severity::Warning,
                format!(
                    "'{}::{}' only reports an error, refusing '{}::{}'. Anything holding a base handle now fails at run \
                     time. Move the shared part into a common base and give each variant only \
                     what it supports.",
                    cls.name, m.name, classes.list[k].name, classes.list[k].methods[bi].name
                ),
                c.loc(m.tok),
            );
        }
    }
}

fn only_reports(c: &Cst, body: NodeId) -> bool {
    fn ok(c: &Cst, n: NodeId) -> bool {
        match stmt(c, n) {
            Some(St::Expr) => {
                if let Some(e) = exprs(c, n).into_iter().next().map(|e| strip_parens(c, e)) {
                    if c.kind(e) == CallExpr
                        && (report_level(c, e) != Level::None || call_name(c, e).is_some_and(|x| x.starts_with('$')))
                    {
                        return true;
                    }
                }
                is_report_scaffolding(c, n)
            }
            Some(St::If) => is_report_guard(c, n) && nested_stmts(c, n).into_iter().all(|x| ok(c, x)),
            Some(St::Decl) => is_report_scaffolding(c, n),
            Some(St::Return | St::Timed) => false,
            Some(St::Seq | St::Par(_)) => list(c, n).into_iter().all(|x| ok(c, x)),
            Some(_) => nested_stmts(c, n).into_iter().all(|x| ok(c, x)),
            None => true,
        }
    }
    list(c, body).into_iter().all(|x| ok(c, x))
}
