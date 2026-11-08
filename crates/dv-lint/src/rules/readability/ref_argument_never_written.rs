//! SV-API-001 (RefArgumentNeverWritten): Flags ref arguments that the routine
//! only reads; declare them const ref so callers know they are not modified
//! (clang-tidy readability-non-const-parameter).

use sv_syntax::{NodeId, SyntaxKind};
use crate::support::class_index::{written_names, Classes};
use crate::support::routine_metrics::user_routines;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;

/// The routine declaration whose name token is `tok`.
/// Routine declarations by the last token of their name (the first
/// function, then task, of a token).
fn routine_decls(c: &Cst) -> std::collections::HashMap<u32, NodeId> {
    let mut out = std::collections::HashMap::new();
    for &d in c.kind_nodes(FunctionDecl).iter().chain(c.kind_nodes(TaskDecl)) {
        if let Some((_, b)) = c.tree.child(d, Name).and_then(|n| c.tree.token_bounds(n)) {
            out.entry(b).or_insert(d);
        }
    }
    out
}

/// The name token of port `name` in a routine's port list.
fn port_tok(c: &Cst, decl: NodeId, name: &str) -> Option<u32> {
    let list = c.tree.child(decl, PortList)?;
    for p in c.tree.children(list).filter(|&p| c.kind(p) == Port) {
        for d in c.tree.children(p).filter(|&d| c.kind(d) == Declarator) {
            let nm = c.tree.child(d, Name)?;
            if c.tree.bare_text(nm) == name {
                return c.tree.token_bounds(nm).map(|(a, _)| a);
            }
        }
    }
    None
}

/// Non-const ref formals. As in slang, a formal that writes its own type
/// without a direction is an input again (`ref int q[$], int v`).
fn ref_ports(c: &Cst, decl: NodeId) -> Vec<String> {
    let Some(list) = c.tree.child(decl, PortList) else { return Vec::new() };
    let mut out = Vec::new();
    let mut is_ref = false;
    let mut is_const = false;
    for p in c.tree.children(list).filter(|&p| c.kind(p) == Port) {
        let toks: Vec<&str> = c.toks(p).map(|t| c.text_of(t)).collect();
        if let Some(d) = toks.iter().find(|t| matches!(**t, "input" | "output" | "inout" | "ref")) {
            is_ref = *d == "ref";
            is_const = toks.contains(&"const");
        } else if c.tree.child(p, DataType).is_some() {
            is_ref = false;
        }
        if !is_ref || is_const {
            continue;
        }
        for d in c.tree.children(p).filter(|&d| c.kind(d) == Declarator) {
            if let Some(nm) = c.tree.child(d, Name) {
                out.push(c.tree.bare_text(nm));
            }
        }
    }
    out
}

pub(crate) fn ref_argument_never_written(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    let g = crate::support::class_metrics::globals(c);
    let decls = routine_decls(c);
    for r in user_routines(c, &classes) {
        let Some(body) = r.body else { continue };
        let Some(&decl) = decls.get(&r.tok) else { continue };
        if let Some(i) = r.class {
            let m = classes.list[i].methods.iter().find(|m| m.tok == r.tok);
            if m.is_some_and(|m| m.virtual_kw) || classes.override_of(i, &r.name).is_some() {
                continue;
            }
        }
        let refs = ref_ports(c, decl);
        if refs.is_empty() {
            continue;
        }
        let env = crate::support::class_metrics::Env::new(c, &classes, r.class, Some(decl), &g);
        let written = written_names(c, &env, body, true);
        for a in refs {
            if written.contains(&a) {
                continue;
            }
            let Some(tok) = port_tok(c, decl, &a) else { continue };
            for _ in 0..r.times {
                s.add(
                    "SV-API-001",
                    "RefArgumentNeverWritten",
                    Severity::Info,
                    format!(
                        "'{a}' is passed by ref to '{}' but never modified. Declare it 'const ref' so callers can see the routine only reads it.",
                        r.name
                    ),
                    c.loc(tok),
                );
            }
        }
    }
}
