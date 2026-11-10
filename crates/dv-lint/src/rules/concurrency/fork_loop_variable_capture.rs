//! SV-CON-012 (ForkLoopVariableCapture): Disallows referencing loop variables
//! directly inside 'fork ... join_none' or 'join_any' blocks without an
//! automatic local variable copy. The copy may sit in the fork's own
//! declarations, which the parent thread initializes. The C++ rule is the same.

use crate::support::for_loops::{for_parts, for_vars, is_ident_name, keyword_is, kids, subtree};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::HashSet;
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

/// The names slang reads as `IdentifierName`s under `n`, in order, with
/// the names `DataDeclaration`s declared before each: plain names, member
/// names and user type names, but not the name before a select.
fn ident_names(c: &Cst, n: NodeId, locals: &mut HashSet<String>, out: &mut HashSet<String>) {
    let select_base =
        |x: NodeId| c.parent(x).is_some_and(|p| c.kind(p) == SelectExpr && kids(c, p).first() == Some(&x));
    match c.kind(n) {
        DataDecl => {
            for d in kids(c, n).into_iter().filter(|&d| c.kind(d) == Declarator) {
                if let Some(nm) = c.tree.child(d, Name) {
                    locals.insert(c.tree.bare_text(nm));
                }
            }
        }
        NameExpr if is_ident_name(c, n) && !select_base(n) => {
            let name = c.tree.bare_text(n);
            if !locals.contains(&name) {
                out.insert(name);
            }
        }
        MemberExpr if !select_base(n) => {
            if let Some((_, b)) = c.tree.token_bounds(n) {
                let name = c.text_of(b).to_string();
                if !locals.contains(&name) {
                    out.insert(name);
                }
            }
        }
        DataType => {
            if let Some((a, b)) = c.tree.token_bounds(n) {
                let t = c.text_of(a);
                if a == b && t.starts_with(|ch: char| ch.is_ascii_alphabetic() || ch == '_') && !locals.contains(t) {
                    out.insert(t.to_string());
                }
            }
        }
        _ => {}
    }
    for k in c.tree.children(n) {
        ident_names(c, k, locals, out);
    }
}

pub(crate) fn fork_loop_capture(c: &Cst, s: &mut Sink) {
    let mut loops = Vec::new();
    for l in c.nodes(LoopStmt) {
        let vars: Vec<String> = if keyword_is(c, l, "for") {
            let (init, _, _) = for_parts(c, l);
            for_vars(c, &init, true)
        } else if keyword_is(c, l, "foreach") {
            let toks: Vec<u32> = c.toks(l).collect();
            let open = toks.iter().position(|&t| c.text_of(t) == "[");
            open.map(|o| {
                toks[o..]
                    .iter()
                    .map(|&t| c.text_of(t))
                    .filter(|t| t.starts_with(|ch: char| ch.is_ascii_alphabetic() || ch == '_'))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
        } else {
            continue;
        };
        if let (false, Some(&body)) = (vars.is_empty(), kids(c, l).last()) {
            loops.push((vars, body));
        }
    }
    for (vars, body) in loops {
        let mut all = Vec::new();
        subtree(c, body, &mut all);
        for b in all.into_iter().filter(|&b| c.kind(b) == ParBlock) {
            let join = c.last_text(b);
            if join != "join_none" && join != "join_any" {
                continue;
            }
            let mut ids = HashSet::new();
            let mut locals = HashSet::new();
            for item in kids(c, b) {
                // The fork's own declarations are initialized by the parent
                // thread on entry, before any process is spawned (IEEE 1800
                // § 9.3.2): `automatic int k = i;` there is the copy the rule
                // asks for, not a read from a child thread.
                if c.kind(item) == DataDecl {
                    for d in kids(c, item).into_iter().filter(|&d| c.kind(d) == Declarator) {
                        if let Some(nm) = c.tree.child(d, Name) {
                            locals.insert(c.tree.bare_text(nm));
                        }
                    }
                    continue;
                }
                ident_names(c, item, &mut locals, &mut ids);
            }
            // The C++ rule walks a std::unordered_set, which for a few names in
            // distinct buckets yields the last inserted first.
            let Some(var) = vars.iter().rev().find(|v| ids.contains(*v)) else { continue };
            let Some(t) = c.first_tok(b) else { continue };
            s.add(
                "SV-CON-012",
                "ForkLoopVariableCapture",
                Severity::Error,
                format!(
                    "Loop variable '{var}' is referenced directly inside a 'fork ... {join}' child thread without an isolating local variable copy. \
                     In SystemVerilog (IEEE 1800 § 9.3.2), threads spawned \
                     with join_none or join_any execute after the loop advances, causing all spawned child threads to read \
                     the final mutated value of the loop variable. Declare an automatic local variable copy \
                     (e.g. 'automatic int _{var} = {var};') inside the fork block to bind the per-iteration value."
                ),
                c.loc(t),
            );
        }
    }
}
