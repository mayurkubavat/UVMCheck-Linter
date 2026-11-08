//! Assertion and coverage parts (bins, options, action blocks), for the SVA
//! and coverage rules.

use std::collections::HashSet;

use dv_model::Loc;

use sv_syntax::{NodeId, SyntaxKind};

use crate::support::slang_ast::*;

use crate::support::procedural::is_stmt;

use crate::syntax::Cst;


use crate::{Severity, Sink};

use sv_syntax::SyntaxKind::*;

pub(crate) fn add_n(s: &mut Sink, n: usize, rule: &'static str, name: &'static str, sev: Severity, msg: String, loc: Loc) {
    for _ in 0..n {
        s.add(rule, name, sev, msg.clone(), loc);
    }
}

pub(crate) fn label_of(c: &Cst, n: NodeId) -> Option<String> {
    let l = c.tree.child(n, StmtLabel)?;
    c.toks(l).next().map(|t| c.text_of(t).to_string())
}

/// A bins declaration: its keyword (`bins`, `illegal_bins`, `ignore_bins`)
/// and name.
pub(crate) fn bins_kw<'a>(c: &'a Cst, b: NodeId) -> &'a str {
    c.toks(b).map(|t| c.text_of(t)).find(|t| matches!(*t, "bins" | "illegal_bins" | "ignore_bins")).unwrap_or("")
}

pub(crate) fn bins_name(c: &Cst, b: NodeId) -> Option<(String, u32)> {
    let n = c.tree.child(b, Name)?;
    let t = c.first_tok(n)?;
    Some((c.text_of(t).to_string(), t))
}

/// The values of a `{ ... }` bins initializer.
pub(crate) fn bins_values(c: &Cst, b: NodeId) -> Vec<NodeId> {
    match c.tree.child(b, RangeList) {
        Some(r) => kids(c, r),
        None => Vec::new(),
    }
}

/// `option.x = v` of a coverage option: the assignment.
pub(crate) fn option_parts(c: &Cst, o: NodeId) -> Option<(NodeId, NodeId)> {
    let e = *kids(c, o).first()?;
    if c.kind(e) != BinaryExpr || !c.keyword(e).is_some_and(|t| c.text_of(t) == "=") {
        return None;
    }
    let k = kids(c, e);
    Some((e, *k.get(1)?))
}

/// What a coverage option belongs to: its coverpoint's or cross's label (or
/// `unnamed_cp` / `unnamed_cross`), else its covergroup's name.
pub(crate) fn option_target(c: &Cst, o: NodeId) -> String {
    for a in ancestors(c, o) {
        match c.kind(a) {
            Coverpoint => return label_of(c, a).unwrap_or_else(|| "unnamed_cp".into()),
            Cross => return label_of(c, a).unwrap_or_else(|| "unnamed_cross".into()),
            CovergroupDecl => return decl_name(c, a),
            _ => {}
        }
    }
    String::new()
}

/// An immediate assertion's keyword: `assert`, `assume` or `cover`.
pub(crate) fn assert_kw<'a>(c: &'a Cst, a: NodeId) -> &'a str {
    c.toks(a).map(|t| c.text_of(t)).find(|t| matches!(*t, "assert" | "assume" | "cover" | "restrict" | "expect")).unwrap_or("")
}

/// An assertion's pass and fail actions (slang's ifTrue and ifFalse).
pub(crate) fn actions(c: &Cst, a: NodeId) -> (Option<NodeId>, Option<NodeId>) {
    let k = kids(c, a);
    let pass = k.iter().copied().find(|&x| is_stmt(c.kind(x)) && c.kind(x) != EmptyStmt);
    let fail = k.iter().copied().find(|&x| c.kind(x) == ElseClause).and_then(|e| kids(c, e).first().copied());
    (pass, fail)
}

/// How often `swe::UserCodeVisitor` reaches a node: never in library code,
/// once per module instance, and once for a parameterized class (its
/// default specialization only).
pub(crate) fn user_code_times(c: &Cst, n: NodeId) -> usize {
    if is_library(c, n) {
        return 0;
    }
    let generic = ancestors(c, n).filter(|&a| c.kind(a) == ClassDecl).count();
    if generic == 0 {
        return times(c, n);
    }
    let unit = ancestors(c, n).find(|&a| matches!(c.kind(a), ModuleDecl | InterfaceDecl | ProgramDecl));
    match unit {
        Some(u) => times(c, u),
        None => 1,
    }
}

/// Assertion statements that slang's AST keeps. A concurrent assertion inside
/// a function is illegal, and an immediate assertion whose condition fails to
/// bind becomes an invalid statement; AST rules never see either. Only one
/// binding failure is modelled: a scoped name (`p::f`) whose head is not a
/// known class, package or typedef (e.g. `uvm_resource_db#(T)::read_by_name`
/// without UVM on the include path).
pub(crate) fn ast_asserts(c: &Cst, kind: SyntaxKind) -> Vec<NodeId> {
    let known = known_scopes(c);
    c.nodes(kind)
        .into_iter()
        .filter(|&a| match kind {
            ConcurrentAssert => !ancestors(c, a).any(|x| c.kind(x) == FunctionDecl),
            _ => !unknown_scope_in_condition(c, a, &known),
        })
        .collect()
}

pub(crate) fn known_scopes(c: &Cst) -> HashSet<String> {
    let mut out: HashSet<String> = ["std", "$unit", "local", "super", "this"].iter().map(|s| s.to_string()).collect();
    for k in [ClassDecl, PackageDecl, TypedefDecl, ForwardTypedef, InterfaceDecl] {
        for n in c.nodes(k) {
            out.insert(last_name(c, n));
        }
    }
    out
}

pub(crate) fn unknown_scope_in_condition(c: &Cst, a: NodeId, known: &HashSet<String>) -> bool {
    kids(c, a)
        .into_iter()
        .filter(|&x| !is_stmt(c.kind(x)) && c.kind(x) != ElseClause)
        .flat_map(|x| subtree(c, x))
        .filter(|&x| c.kind(x) == ScopedExpr)
        .filter_map(|x| kids(c, x).first().copied())
        .filter(|&h| c.kind(h) == NameExpr)
        .any(|h| {
            let head = c.toks(h).next().map(|t| c.text_of(t).to_string()).unwrap_or_default();
            !head.is_empty() && !known.contains(&head)
        })
}
