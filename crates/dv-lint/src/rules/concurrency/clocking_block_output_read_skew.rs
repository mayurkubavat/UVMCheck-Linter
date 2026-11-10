//! SV-CB-001 (ClockingBlockOutputReadSkew): Disallows reading output signals of
//! a clocking block. The target of a drive is not a read. The C++ rule is the
//! same.

use crate::support::procedural::start;
use crate::support::slang_ast::kids;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::HashSet;
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

/// The declarations of output-only clocking items anywhere in the compile,
/// as written (`d`, `h = x.y`).
fn clocking_outputs(c: &Cst) -> std::rc::Rc<HashSet<String>> {
    thread_local! {
        static NAMES: crate::shared::CompileCache<HashSet<String>> = const { std::cell::RefCell::new(None) };
    }
    let key = c.compile_key();
    if let Some(v) = NAMES.with(|k| k.borrow().as_ref().filter(|e| e.0 == key).map(|e| e.1.clone())) {
        return v;
    }
    let mut out = HashSet::new();
    for (i, t) in c.all.iter().enumerate() {
        if !t.nodes.iter().any(|n| n.kind == ClockingItem) {
            continue;
        }
        let mut own;
        let tc = if std::ptr::eq(t, c.tree) {
            c
        } else {
            own = Cst::new(t, i, svlang::semantic::Locator::default());
            own.all = c.all;
            &own
        };
        for item in tc.nodes(ClockingItem) {
            let toks: Vec<&str> = tc.toks(item).map(|t| tc.text_of(t)).collect();
            if !toks.contains(&"output") || toks.contains(&"input") {
                continue;
            }
            for d in tc.tree.children(item).filter(|&d| tc.kind(d) == Declarator) {
                let name = crate::support::slang_ast::slang_trimmed(tc, d);
                if !name.is_empty() {
                    out.insert(name);
                }
            }
        }
    }
    let v = std::rc::Rc::new(out);
    NAMES.with(|k| *k.borrow_mut() = Some((key, v.clone())));
    v
}

pub(crate) fn clocking_output_read(c: &Cst, out: &mut Sink) {
    let outputs = clocking_outputs(c);
    let mut check = |e: NodeId| {
        let text = crate::support::slang_ast::slang_trimmed(c, e);
        let at = match text.find(".cb.") {
            Some(p) => p + 4,
            None if text.starts_with("cb.") => 3,
            None => return,
        };
        let sig = &text[at..];
        let sig = match sig.find(|ch: char| " .[]()+-*/=!&|^".contains(ch)) {
            Some(d) => &sig[..d],
            None => sig,
        };
        if sig.is_empty() || !(outputs.contains(sig) || sig == "data_out" || sig == "tx_data") {
            return;
        }
        let Some(loc) = start(c, e) else { return };
        out.add(
            "SV-CB-001",
            "ClockingBlockOutputReadSkew",
            Severity::Warning,
            format!(
                "Reading clocking block output signal '{text}'. In SystemVerilog (IEEE 1800 § 14.16), clocking block output signals \
                 are drive-only; reading an output clocking signal observes post-drive values with simulation race conditions. Read \
                 the corresponding input or interface signal instead."
            ),
            loc,
        );
    };
    for n in c.nodes(BinaryExpr) {
        // The target of a drive (`vif.cb.data <= x`, `cb.data = x`) is
        // written, not read.
        let op = c.keyword(n).map(|t| c.text_of(t)).unwrap_or("");
        let drive = op == "=" || (op == "<=" && c.parent(n).is_some_and(|p| c.kind(p) == ExprStmt));
        for k in kids(c, n).into_iter().take(2).skip(usize::from(drive)) {
            check(k);
        }
    }
    for n in c.nodes(IfStmt) {
        if let (Some(cond), _, _) = crate::support::procedural::if_parts(c, n) {
            check(cond);
        }
    }
}
