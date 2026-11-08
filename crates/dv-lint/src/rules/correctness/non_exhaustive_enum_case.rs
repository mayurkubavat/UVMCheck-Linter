//! SV-FSM-001 (NonExhaustiveEnumCase): Case statement switching over an enum
//! type does not explicitly handle all declared enum variants while using a
//! default: arm.

use crate::support::literal_values::{decl_of_expr, literal_value};
use crate::support::literals::ast_times;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

/// The enum a declaration's type is, through typedefs: its name as slang
/// prints the canonical type, and its members with their values.
fn enum_of(c: &Cst, dt: NodeId) -> Option<(String, Vec<(String, Option<i64>)>)> {
    let mut dt = dt;
    let mut name = String::new();
    for _ in 0..4 {
        if c.tok(dt, "enum").is_some() {
            let mut members = Vec::new();
            let mut next = Some(0i64);
            for d in c.tree.children(dt).filter(|&d| c.kind(d) == Declarator) {
                let Some(nm) = c.tree.child(d, Name) else { continue };
                let v = match c.tree.children(d).find(|&x| x != nm && c.kind(x) != Dim) {
                    Some(e) => literal_value(c, c.strip(e)),
                    None => next,
                };
                members.push((c.tree.bare_text(nm), v));
                next = v.map(|v| v + 1);
            }
            return Some((name, members));
        }
        let toks: Vec<u32> = c.toks(dt).collect();
        let [t] = toks[..] else { return None };
        let tn = c.text_of(t);
        // The typedef in scope: in a unit around the use (innermost first),
        // else in a package or at the top of the file.
        let scope_of = |n: NodeId| {
            std::iter::successors(c.parent(n), |&a| c.parent(a)).find(|&a| {
                matches!(
                    c.kind(a),
                    ModuleDecl | InterfaceDecl | ProgramDecl | ClassDecl | PackageDecl | FunctionDecl | TaskDecl
                )
            })
        };
        let around: Vec<NodeId> = std::iter::successors(c.parent(dt), |&a| c.parent(a)).collect();
        let td = c
            .nodes(TypedefDecl)
            .into_iter()
            .filter(|&td| c.tree.child(td, Name).is_some_and(|n| c.tree.bare_text(n) == tn))
            .filter_map(|td| match scope_of(td) {
                None => Some((usize::MAX, td)),
                Some(sc) => match around.iter().position(|&a| a == sc) {
                    Some(depth) => Some((depth, td)),
                    None if c.kind(sc) == PackageDecl => Some((usize::MAX - 1, td)),
                    None => None,
                },
            })
            .min_by_key(|x| x.0)
            .map(|x| x.1)?;
        name = tn.to_string();
        dt = c.tree.child(td, DataType)?;
    }
    None
}

pub(crate) fn non_exhaustive_enum_case(c: &Cst, s: &mut Sink) {
    for cs in c.nodes(CaseStmt) {
        if !c.toks(cs).any(|t| matches!(c.text_of(t), "case" | "casex" | "casez")) {
            continue;
        }
        let parts = crate::support::routine_bodies::case_parts(c, cs);
        let Some(e) = parts.expr else { continue };
        let e = c.strip(e);
        let dtype = if c.kind(e) == CallExpr {
            // `case (get_endian())`: the routine's return type.
            crate::support::slang_ast::callee_decl(c, e)
                .and_then(|r| c.tree.child(r, ReturnType))
                .and_then(|r| c.tree.child(r, DataType))
        } else {
            let Some((d, _)) = decl_of_expr(c, e) else { continue };
            if c.kind(d.node) == Declarator && c.tree.children(d.node).any(|x| c.kind(x) == Dim) {
                continue;
            }
            d.dtype
        };
        let Some((name, members)) = dtype.and_then(|t| enum_of(c, t)) else { continue };
        if !parts.items.iter().any(|(_, _, def)| *def) {
            continue;
        }
        let mut handled = std::collections::HashSet::new();
        let mut labels_ok = true;
        for (labels, _, _) in &parts.items {
            for &l in labels {
                let l = c.strip(l);
                if c.kind(l) == NameExpr {
                    handled.insert(c.tree.bare_text(l));
                } else {
                    // A constant label compares the enum's value with an
                    // int; uvmcheck reports nothing for such a case.
                    labels_ok = false;
                }
            }
        }
        if !labels_ok {
            continue;
        }
        let names: Vec<&str> = members.iter().map(|(m, _)| m.as_str()).collect();
        let Some(order) = crate::support::slang_ast::std_unordered_order(&names) else { continue };
        let missing: Vec<String> =
            order.into_iter().filter(|m| !handled.contains(m)).map(|m| format!("'{m}'")).collect();
        if missing.is_empty() {
            continue;
        }
        for _ in 0..ast_times(c, cs) {
            s.add(
                "SV-FSM-001",
                "NonExhaustiveEnumCase",
                Severity::Warning,
                format!(
                    "Case statement switching over enum '{name}' does not explicitly handle variant(s): {}. Using a catch-all 'default:' arm silently masks unhandled states and allows specification drift bugs to pass undetected. Explicitly enumerate all variants or remove 'default:' to enforce full pattern exhaustiveness.",
                    missing.join(", ")
                ),
                c.loc_of(cs),
            );
        }
    }
}
