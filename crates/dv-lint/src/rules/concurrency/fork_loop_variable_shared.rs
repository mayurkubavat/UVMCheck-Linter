//! SV-THR-001 (ForkLoopVariableShared): Spawning background threads with
//! fork..join_none inside loops without copying the loop variable to an
//! automatic local variable causes all threads to see the final loop index.

use std::collections::HashSet;
use sv_syntax::{Child, NodeId, SyntaxKind};
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::procedural::{Sym, add_n, assign_lhs, if_parts, is_stmt, items, join_kind, loop_body, loop_kw, name_sym, timing_of};

pub(crate) fn fork_loop_variable(c: &Cst, s: &mut Sink) {
    for l in c.nodes(LoopStmt) {
        if loop_kw(c, l) != "for" {
            continue;
        }
        let Some(body) = loop_body(c, l) else { continue };
        // Loop variables: declared in the initializer or assigned there.
        let mut vars: HashSet<u32> = HashSet::new();
        let mut names: HashSet<String> = HashSet::new();
        let mut semis = 0;
        for k in c.kids(l) {
            match *k {
                Child::Token(t) if c.text_of(t) == ";" => semis += 1,
                Child::Node(x) if semis == 0 => {
                    if c.kind(x) == DataDecl {
                        for d in c.tree.children(x).filter(|&d| c.kind(d) == Declarator) {
                            vars.insert(d.0);
                        }
                    } else if let Some(lhs) = assign_lhs(c, x) {
                        if c.kind(lhs) == NameExpr {
                            match name_sym(c, lhs) {
                                Some((Sym::Decl(d), _, _)) => {
                                    vars.insert(d);
                                }
                                Some((_, nm, _)) => {
                                    names.insert(nm);
                                }
                                None => {}
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if vars.is_empty() && names.is_empty() {
            continue;
        }
        let n = times(c, l);
        for f in subtree(c, body) {
            if c.kind(f) != ParBlock || !matches!(join_kind(c, f), "join_none" | "join_any") {
                continue;
            }
            // slang visits only the expressions of a one-statement body.
            let it = items(c, f);
            if it.len() != 1 {
                continue;
            }
            let st = it[0];
            for e in stmt_exprs(c, st) {
                for x in subtree(c, e) {
                    if c.kind(x) != NameExpr {
                        continue;
                    }
                    let hit = match name_sym(c, x) {
                        Some((Sym::Decl(d), _, _)) => vars.contains(&d),
                        Some((Sym::Name(nm), _, _)) => names.contains(&nm),
                        _ => false,
                    };
                    if !hit {
                        continue;
                    }
                    let Some(t) = c.first_tok(x) else { continue };
                    let name = c.text_of(t);
                    add_n(
                        s,
                        n,
                        "SV-THR-001",
                        "ForkLoopVariableShared",
                        Severity::Error,
                        format!(
                            "Loop variable '{name}' is captured by reference inside a fork..join_none/any block. \
                             Copy to an automatic local variable before the fork to prevent race conditions."
                        ),
                        c.loc(t),
                    );
                }
            }
        }
    }
}

/// The expressions a statement holds itself (not those of the statements
/// inside it): what slang's `visitExprs` reaches.
fn stmt_exprs(c: &Cst, st: NodeId) -> Vec<NodeId> {
    match c.kind(st) {
        ExprStmt => kids(c, st).into_iter().filter(|&x| !matches!(c.kind(x), StmtLabel | Attribute)).collect(),
        IfStmt => if_parts(c, st).0.into_iter().collect(),
        ReturnStmt | EventTrigger | DisableStmt => kids(c, st),
        LoopStmt | DoWhileStmt => kids(c, st).into_iter().filter(|&x| !is_stmt(c.kind(x)) && !matches!(c.kind(x), StmtLabel | Attribute)).collect(),
        WaitStmt | CaseStmt => kids(c, st).into_iter().filter(|&x| !is_stmt(c.kind(x)) && !matches!(c.kind(x), StmtLabel | Attribute)).collect(),
        TimedStmt => timing_of(c, st).into_iter().collect(),
        _ => Vec::new(),
    }
}
