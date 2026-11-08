//! SV-SHD-002 (ForeachLoopVariableShadowing): Flags loop index variables in
//! 'foreach (arr[i])' loops that shadow an existing class property or routine
//! local variable.

use crate::support::class_items::{class_items, declarators, kids};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::HashSet;
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

/// The loop variables of a `foreach (a[i, j])` header: names and tokens.
fn foreach_vars(c: &Cst, l: NodeId) -> Vec<(String, u32)> {
    let toks: Vec<u32> = c.toks(l).collect();
    // The last top-level `[` before the closing `)`.
    let close = toks.iter().rposition(|&t| c.text_of(t) == ")");
    let Some(close) = close else { return Vec::new() };
    let Some(open) = toks[..close].iter().rposition(|&t| c.text_of(t) == "[") else { return Vec::new() };
    let mut out = Vec::new();
    let mut cur: Option<u32> = None;
    for &t in &toks[open + 1..close] {
        match c.text_of(t) {
            "," | "]" => {
                if let Some(v) = cur.take() {
                    out.push((c.text_of(v).to_string(), v));
                }
            }
            _ => cur = Some(t),
        }
    }
    out
}

fn shadow_walk(c: &Cst, n: NodeId, props: &mut HashSet<String>, locals: &mut HashSet<String>, s: &mut Sink) {
    match c.kind(n) {
        ClassDecl => {
            let prev = std::mem::take(props);
            for d in class_items(c, n).into_iter().filter(|&x| c.kind(x) == DataDecl) {
                for (_, v, _) in declarators(c, d) {
                    props.insert(v);
                }
            }
            for x in kids(c, n) {
                shadow_walk(c, x, props, locals, s);
            }
            *props = prev;
            return;
        }
        FunctionDecl | TaskDecl => {
            let prev = std::mem::take(locals);
            if let Some(pl) = c.tree.child(n, PortList) {
                for p in c.tree.children(pl).filter(|&x| c.kind(x) == Port) {
                    for (_, v, _) in declarators(c, p) {
                        locals.insert(v);
                    }
                }
            }
            if let Some(body) = c.tree.child(n, Body) {
                for d in c.tree.children(body).filter(|&x| c.kind(x) == DataDecl) {
                    for (_, v, _) in declarators(c, d) {
                        locals.insert(v);
                    }
                }
            }
            for x in kids(c, n) {
                shadow_walk(c, x, props, locals, s);
            }
            *locals = prev;
            return;
        }
        LoopStmt if c.keyword(n).is_some_and(|k| c.text_of(k) == "foreach") => {
            for (v, at) in foreach_vars(c, n) {
                let msg = if props.contains(&v) {
                    format!(
                        "Foreach loop index variable '{v}' shadows class property '{v}'. Per IEEE 1800 § 12.7.3, foreach loop variables are implicitly declared in the local loop scope and shadow outer identifiers with the same name, masking class state. Rename the loop index (e.g. '{v}_idx')."
                    )
                } else if locals.contains(&v) {
                    format!(
                        "Foreach loop index variable '{v}' shadows local variable '{v}' declared in the enclosing routine. Per IEEE 1800 § 12.7.3, foreach loop variables create a nested scope, masking routine variables. Rename the loop index (e.g. '{v}_idx')."
                    )
                } else {
                    continue;
                };
                s.add("SV-SHD-002", "ForeachLoopVariableShadowing", Severity::Warning, msg, c.loc(at));
            }
        }
        _ => {}
    }
    for x in kids(c, n) {
        shadow_walk(c, x, props, locals, s);
    }
}

pub(crate) fn foreach_shadowing(c: &Cst, s: &mut Sink) {
    let (mut props, mut locals) = (HashSet::new(), HashSet::new());
    shadow_walk(c, c.tree.root, &mut props, &mut locals, s);
}
