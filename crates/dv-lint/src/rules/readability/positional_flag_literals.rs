//! SV-RDB-001 (PositionalFlagLiterals): Flags calls that pass two or more 0/1
//! literals positionally to 1-bit arguments; use named argument binding
//! (clang-tidy bugprone-argument-comment, Clean Code flag arguments).

use crate::support::config_db_calls::{bool_literal, user_visits};
use crate::support::slang_ast::kids;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

/// The `min_flags` default.
const MIN_FLAGS: usize = 2;

/// A call of a user routine passing two or more 0/1 literals positionally
/// to one-bit formals.
pub(crate) fn positional_flags(c: &Cst, out: &mut Sink) {
    let one_bit = crate::expr_types::one_bit_formals(c);
    let ix = c.index();
    for call in c.nodes(CallExpr) {
        let Some(&callee) = kids(c, call).first() else { continue };
        if c.first_tok(callee).is_some_and(|t| c.text_of(t).starts_with('$')) {
            continue;
        }
        let Some(decl) = crate::support::slang_ast::callee_decl(c, call) else { continue };
        if crate::support::slang_ast::is_library(c, decl) {
            continue;
        }
        let times = user_visits(c, call);
        if times == 0 {
            continue;
        }
        // Formals in order.
        let formals: Vec<NodeId> = c
            .tree
            .child(decl, PortList)
            .map(|pl| {
                c.tree
                    .children(pl)
                    .filter(|&p| c.kind(p) == Port)
                    .flat_map(|p| c.tree.children(p).filter(|&d| c.kind(d) == Declarator).collect::<Vec<_>>())
                    .collect()
            })
            .unwrap_or_default();
        // Leading positional arguments.
        let Some(al) = c.tree.child(call, ArgList) else { continue };
        let mut names = Vec::new();
        for (i, a) in c.tree.children(al).filter(|&a| c.kind(a) == Arg).enumerate() {
            if c.tok(a, ".").is_some() {
                break;
            }
            let (Some(&f), Some(&e)) = (formals.get(i), kids(c, a).last()) else { continue };
            if one_bit.contains(&(ix, f)) && bool_literal(c, e).is_some() {
                names.push(c.tree.child(f, Name).map(|n| c.tree.bare_text(n)).unwrap_or_default());
            }
        }
        if names.len() < MIN_FLAGS {
            continue;
        }
        let named: Vec<String> = names.iter().map(|n| format!(".{n}(...)")).collect();
        let name = crate::support::slang_ast::last_name(c, decl);
        let Some(t) = c.first_tok(call) else { continue };
        for _ in 0..times {
            out.add(
                "SV-RDB-001",
                "PositionalFlagLiterals",
                Severity::Info,
                format!(
                    "Call to '{name}' passes {} positional 0/1 flags. A reader cannot tell which is which, and adding an argument shifts them silently; bind them by name: {}.",
                    names.len(),
                    named.join(", ")
                ),
                c.loc(t),
            );
        }
    }
}
