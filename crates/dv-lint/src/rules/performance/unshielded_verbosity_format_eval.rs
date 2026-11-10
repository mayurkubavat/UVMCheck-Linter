//! SV-PRF-005 (UnshieldedVerbosityFormatEval): Calling $sformatf inside
//! uvm_info/warning without checking uvm_report_enabled formats strings even
//! when messages are discarded by verbosity.

use crate::support::ast::{call_site, int_value, source_text, unparen, Ast, CallSite};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;
use std::collections::HashMap;
use svlang::syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

fn unshielded_format(c: &Cst, a: &Ast, s: &mut Sink) {
    fn walk(c: &Cst, a: &Ast, t: &Formals, n: NodeId, guard: usize, s: &mut Sink) {
        let mut guard = guard;
        if c.kind(n) == IfStmt
            && c.tree.children(n).next().is_some_and(|cond| source_text(c, cond).contains("uvm_report_enabled"))
        {
            guard += 1;
        }
        // Only a call named like uvm_info/uvm_warning can fire: test the
        // name before building the call site's arguments.
        let named = c.kind(n) == CallExpr
            && c.tree.children(n).next().is_some_and(|callee| {
                let t = c.last_text(callee);
                ["uvm_report_info", "uvm_info", "uvm_report_warning", "uvm_warning"].iter().any(|w| t.contains(w))
            });
        if guard == 0 && named {
            if let Some(call) = call_site(c, n) {
                if check(c, t, &call, s) {
                    a.again(s, c, n);
                }
            }
        }
        for k in c.tree.children(n) {
            walk(c, a, t, k, guard, s);
        }
    }
    fn check(c: &Cst, table: &Formals, call: &CallSite, s: &mut Sink) -> bool {
        let n = &call.name;
        let info_or_warning =
            ["uvm_report_info", "uvm_info", "uvm_report_warning", "uvm_warning"].iter().any(|w| n.contains(w));
        if !info_or_warning {
            return false;
        }
        // slang's arguments() has one per formal, defaults filled in.
        let ports = table.get(n.as_str()).cloned().unwrap_or_default();
        if ports.len().max(call.args.len()) < 2 {
            return false;
        }
        let Some(msg) = call.args.get(1).copied().flatten() else { return false };
        if !source_text(c, msg).contains("$sformatf") {
            return false;
        }
        // The verbosity: written, or the formal's default. An enum
        // constant bound to an int formal is a conversion with no syntax,
        // so in practice only a plain decimal literal counts.
        let verb = call.args.get(2).copied().flatten().or_else(|| ports.get(2).copied().flatten());
        let high = verb.is_some_and(|v| {
            let v = unparen(c, v);
            let toks = c.tree.token_range(v);
            c.kind(v) == Literal
                && toks.len() == 1
                && c.text_of(toks[0]).bytes().all(|b| b.is_ascii_digit() || b == b'_')
                && int_value(c, v).is_some_and(|x| x > 200)
        });
        if !high {
            return false;
        }
        let Some(at) = c.first_tok(msg) else { return false };
        s.add(
            "SV-PRF-005",
            "UnshieldedVerbosityFormatEval",
            Severity::Warning,
            "Calling $sformatf inside uvm_info/warning with high verbosity without checking 'uvm_report_enabled'. \
             Because macro/subroutine arguments evaluate eagerly, expensive string formatting and heap allocations execute \
             on every transaction even when messages are suppressed by simulation verbosity. \
             Wrap call in 'if (uvm_report_enabled(...))' or use `uvm_info_begin/end."
                .into(),
            c.loc(at),
        );
        true
    }
    walk(c, a, &formals(c), c.tree.root, 0, s);
}

pub(crate) fn ast_unshielded_format(d: &Design, csts: &[Cst], s: &mut Sink) {
    for c in csts {
        unshielded_format(c, &Ast::new(d, c), s);
    }
}

/// The default value of each formal of the subroutines named `name` in the
/// tree (the first declaration found), `None` for a formal without one.
/// For each subroutine name, the default of each formal: the first
/// function, then task, then prototype of that name in the tree.
type Formals<'t> = HashMap<&'t str, Vec<Option<NodeId>>>;

fn formals<'t>(c: &Cst<'t>) -> Formals<'t> {
    let mut out = HashMap::new();
    for kind in [FunctionDecl, TaskDecl, MethodPrototype] {
        for n in c.nodes(kind) {
            let Some(nm) = c.tree.child(n, Name) else { continue };
            out.entry(c.last_text(nm)).or_insert_with(|| {
                let Some(list) = c.tree.child(n, PortList) else { return Vec::new() };
                c.tree
                    .children(list)
                    .filter(|&p| c.kind(p) == Port)
                    .map(|p| {
                        let d = c.tree.child(p, Declarator)?;
                        let eq = c.tok(d, "=")?;
                        c.node_after(d, eq)
                    })
                    .collect()
            });
        }
    }
    out
}
