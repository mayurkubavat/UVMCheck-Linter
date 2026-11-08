//! SV-BUG-007 (SuspiciousArgumentOrder): Flags positional calls where an
//! argument's name matches a different parameter of the callee, such as
//! write(len, addr) for write(addr, len) (clang-tidy
//! readability-suspicious-call-argument, Sonar S2234).

use crate::support::class_items::{declarators, user_code};
use crate::support::data_items::unparen;
use crate::support::real_types::Types;
use crate::support::sema_facts::ast_ok;
use crate::support::type_text::expr_text;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::HashMap;
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

/// The formal arguments of a routine, in order: name and declared type
/// text (carried over from the previous port when left out).
fn formals(c: &Cst, f: NodeId) -> Vec<(String, Option<String>)> {
    let Some(pl) = c.tree.child(f, PortList) else { return Vec::new() };
    let mut out = Vec::new();
    let mut last: Option<String> = Some("logic".into());
    for p in c.tree.children(pl).filter(|&p| c.kind(p) == Port) {
        if let Some(dt) = c.tree.child(p, DataType) {
            last = Some(c.tree.bare_text(dt));
        } else if c.toks(p).next().is_some() {
            last = Some("logic".into());
        }
        for (dcl, v, _) in declarators(c, p) {
            let dims = c.tree.children(dcl).any(|k| c.kind(k) == Dim);
            out.push((v, if dims { None } else { last.clone() }));
        }
    }
    out
}

/// The positional arguments of a call, or `None` when any is named or empty.
fn ordered_args(c: &Cst, call: NodeId) -> Option<Vec<NodeId>> {
    let list = c.tree.child(call, ArgList)?;
    let args: Vec<NodeId> = c.tree.children(list).filter(|&a| c.kind(a) == Arg).collect();
    if args.len() != c.toks(list).filter(|&t| c.text_of(t) == ",").count() + 1 {
        return None;
    }
    args.iter()
        .map(|&a| {
            if c.tok(a, ".").is_some() {
                return None;
            }
            c.tree.children(a).next()
        })
        .collect()
}

/// swe::referencedSymbol's name: a variable or the member accessed.
fn arg_name(c: &Cst, e: NodeId) -> String {
    let e = crate::support::data_items::strip_conv(c, e).0;
    match c.kind(e) {
        NameExpr if !c.tree.bare_text(e).starts_with('$') => c.tree.bare_text(e).to_lowercase(),
        MemberExpr => c.last_text(e).to_lowercase(),
        _ => String::new(),
    }
}

pub(crate) fn suspicious_argument_order(c: &Cst, s: &mut Sink) {
    let types = Types::new(c);
    let mut free: HashMap<String, Vec<NodeId>> = HashMap::new();
    for f in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)) {
        if c.parent(f).is_some_and(|p| c.kind(p) == ClassDecl) || types.out_of_block_class(f).is_some() {
            continue;
        }
        if let Some(n) = c.tree.child(f, Name) {
            free.entry(c.last_text(n).to_string()).or_default().push(f);
        }
    }
    for call in c.nodes(CallExpr) {
        if !user_code(c, call) {
            continue;
        }
        let Some(args) = ordered_args(c, call) else { continue };
        if args.len() < 2 {
            continue;
        }
        let Some(f) = types.callee(call, &free) else { continue };
        let formals = formals(c, f);
        if formals.len() < 2 {
            continue;
        }
        let lower: Vec<String> = formals.iter().map(|x| x.0.to_lowercase()).collect();
        let mut hit = None;
        'outer: for (i, &a) in args.iter().enumerate().take(formals.len()) {
            let name = arg_name(c, a);
            if name.is_empty() || name == lower[i] {
                continue;
            }
            for (j, l) in lower.iter().enumerate() {
                if j != i && *l == name {
                    hit = Some((i, j, a));
                    break 'outer;
                }
            }
        }
        let Some((i, j, a)) = hit else { continue };
        if !ast_ok(c, call) {
            continue;
        }
        let Some(at) = c.first_tok(unparen(c, a)) else { continue };
        let fname = c.tree.child(f, Name).map(|n| c.last_text(n).to_string()).unwrap_or_default();
        // An argument converted to the formal's type has no text of its own
        // in slang's AST.
        let same = formals[i].1.is_some() && types.arg_type(a) == formals[i].1;
        let text = if same { expr_text(c, a) } else { String::new() };
        s.add(
            "SV-BUG-007",
            "SuspiciousArgumentOrder",
            Severity::Warning,
            format!(
                "'{text}' is passed as argument {} ('{}') of '{fname}', but the routine has a parameter named '{}' at \
                 position {}. The arguments look swapped; check the order or use named arguments.",
                i + 1,
                formals[i].0,
                formals[j].0,
                j + 1
            ),
            c.loc(at),
        );
    }
}
