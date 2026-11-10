//! SV-CON-015 (UnguardedBlockingWait): Flags wait(vif.sig) and @(vif.sig) in
//! drivers and sequences that are not raced against a timeout in fork ...
//! join_any.

use crate::support::procedural::{join_kind, name_sym, own_methods, routine_name, start, timing_kind, timing_of};
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

/// `swe::classRole` reduced to the two roles SV-CON-015 needs.
fn role_of(c: &Cst, cls: NodeId) -> &'static str {
    for n in base_names(c, cls) {
        if !n.starts_with("uvm_") {
            continue;
        }
        return match n.as_str() {
            "uvm_driver" | "uvm_push_driver" => "driver",
            "uvm_sequence" | "uvm_sequence_base" | "uvm_reg_sequence" => "sequence",
            "uvm_monitor" | "uvm_agent" | "uvm_scoreboard" | "uvm_subscriber" | "uvm_env" | "uvm_test"
            | "uvm_report_catcher" | "uvm_sequence_item" | "uvm_transaction" | "uvm_component" | "uvm_root"
            | "uvm_object" | "uvm_void" | "uvm_push_sequencer" | "uvm_mem" | "uvm_vreg" => "other",
            x if x.starts_with("uvm_sequencer")
                || x.starts_with("uvm_reg")
                || x.starts_with("uvm_callback")
                || x.starts_with("uvm_tlm") =>
            {
                "other"
            }
            _ => continue,
        };
    }
    "other"
}

/// The virtual interface type a declaration has: the interface's name.
fn vif_type(c: &Cst, d: Decl) -> Option<String> {
    let dt = d.dtype?;
    let all: Vec<&str> = c.tree.token_range(dt).into_iter().map(|t| c.text_of(t)).collect();
    let toks = all;
    if toks.first() == Some(&"virtual") {
        return toks.iter().skip(1).find(|t| **t != "interface").map(|t| t.to_string());
    }
    // A typedef of a virtual interface.
    let name = *toks.first()?;
    for td in c.nodes(TypedefDecl) {
        if c.tree.child(td, Name).is_some_and(|x| c.tree.bare_text(x) == name) {
            let dt2 = c.tree.child(td, DataType)?;
            let t2: Vec<&str> = c.tree.token_range(dt2).into_iter().map(|t| c.text_of(t)).collect();
            if t2.first() == Some(&"virtual") {
                return t2.iter().skip(1).find(|t| **t != "interface").map(|t| t.to_string());
            }
        }
    }
    None
}

fn is_clocking(c: &Cst, intf: &str, member: &str) -> bool {
    c.nodes(InterfaceDecl)
        .into_iter()
        .filter(|&i| decl_name(c, i) == intf)
        .any(|i| subtree(c, i).into_iter().any(|x| c.kind(x) == ClockingDecl && decl_name(c, x) == member))
}

fn is_exempt_signal(name: &str) -> bool {
    let n = name.to_lowercase();
    ["clk", "clock", "rst", "reset"].iter().any(|k| n.contains(k))
}

/// The interface signal an expression reads first: `Some("")` for a
/// clocking block, clock or reset; `None` when it reads no virtual
/// interface.
fn vif_signal(c: &Cst, e: NodeId) -> Option<String> {
    let mut stack = vec![e];
    while let Some(x) = stack.pop() {
        if c.kind(x) == MemberExpr {
            let is_callee = c.parent(x).is_some_and(|p| c.kind(p) == CallExpr && kids(c, p).first() == Some(&x));
            if !is_callee {
                let mut root = *kids(c, x).first()?;
                while c.kind(root) == MemberExpr {
                    root = *kids(c, root).first()?;
                }
                if c.kind(root) == NameExpr {
                    if let Some((_, _, Some(d))) = name_sym(c, root) {
                        if let Some(intf) = vif_type(c, d) {
                            // Only the member right on the interface handle.
                            let member = c.last_text(x).to_string();
                            let exempt = is_clocking(c, &intf, &member) || is_exempt_signal(&member);
                            return Some(if exempt { String::new() } else { member });
                        }
                    }
                }
            }
        }
        let k: Vec<NodeId> = c.tree.children(x).collect();
        stack.extend(k.into_iter().rev());
    }
    None
}

pub(crate) fn unguarded_wait(c: &Cst, s: &mut Sink) {
    for cls in user_classes(c) {
        if !matches!(role_of(c, cls), "driver" | "sequence") {
            continue;
        }
        let cname = decl_name(c, cls);
        for m in own_methods(c, cls) {
            if c.kind(m) != TaskDecl {
                continue;
            }
            let mname = routine_name(c, m).to_lowercase();
            if mname.contains("timeout") || mname.contains("watchdog") {
                continue;
            }
            for x in subtree(c, m) {
                let guarded = ancestors(c, x)
                    .take_while(|&a| a != m)
                    .any(|a| c.kind(a) == ParBlock && join_kind(c, a) == "join_any");
                if guarded {
                    continue;
                }
                let (expr, how) = match c.kind(x) {
                    WaitStmt
                        if c.keyword(x).is_some_and(|k| c.text_of(k) == "wait")
                            && !c.toks(x).any(|t| c.text_of(t) == "fork") =>
                    {
                        (kids(c, x).into_iter().find(|&y| !matches!(c.kind(y), StmtLabel | Attribute)), "wait")
                    }
                    TimedStmt => {
                        let Some(t) = timing_of(c, x) else { continue };
                        if timing_kind(c, t) != "@" {
                            continue;
                        }
                        let evs: Vec<NodeId> = kids(c, t).into_iter().filter(|&e| c.kind(e) == EventExpr).collect();
                        if evs.len() > 1
                            && evs.iter().any(|&e| {
                                let ex = kids(c, e).first().copied();
                                !ex.and_then(|ex| vif_signal(c, ex)).is_some_and(|sg| !sg.is_empty())
                            })
                        {
                            continue;
                        }
                        (evs.first().and_then(|&e| kids(c, e).first().copied()), "@")
                    }
                    _ => continue,
                };
                let Some(expr) = expr else { continue };
                let Some(sig) = vif_signal(c, expr) else { continue };
                if sig.is_empty() {
                    continue;
                }
                let Some(loc) = start(c, x) else { continue };
                s.add(
                    "SV-CON-015",
                    "UnguardedBlockingWait",
                    Severity::Warning,
                    format!(
                        "{how} on interface signal '{sig}' in '{cname}' has no timeout. If the DUT never answers, the test hangs until the global \
                         timeout with no hint of which agent stalled. Race it against a delay in \
                         fork ... join_any, or call a wait-with-timeout helper."
                    ),
                    loc,
                );
            }
        }
    }
}
