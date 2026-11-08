//! UVM-TST-001 (CheckerCannotFail): Flags scoreboard and checker classes with
//! no way to report a failure: no `uvm_error or `uvm_fatal, no assertion and no
//! comparator, in the class, its bases or the helpers it calls (Sonar S2699,
//! tests without assertions).

use std::collections::{HashMap, HashSet};
use dv_model::{Class, Design};
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;
use crate::support::class_roles::{role, Index, Role};
use crate::syntax::Cst;
use crate::{Severity, Sink};

fn looks_like_checker(d: &Design, cls: &Class) -> bool {
    let r = role(d, cls);
    if r == Role::Scoreboard {
        return true;
    }
    // Names only count for components that receive transactions.
    if r != Role::Component && r != Role::Subscriber {
        return false;
    }
    let n = cls.name.to_lowercase();
    n.contains("scoreboard") || n.contains("checker") || n.ends_with("_scb") || n.contains("_scb_") || n.ends_with("_sb")
}

/// A routine written in the library (uvm_pkg, a uvm_* class, the stub).
fn routine_in_library(d: &Design, c: &Cst, n: NodeId) -> bool {
    let f = d.file_name(c.loc_of(n));
    if f.contains("uvm_mock.svh") || f.contains("uvm_pkg.sv") {
        return true;
    }
    std::iter::successors(c.parent(n), |&a| c.parent(a)).any(|a| {
        matches!(c.kind(a), ClassDecl | PackageDecl)
            && c.tree.child(a, sv_syntax::SyntaxKind::Name).is_some_and(|nm| c.tree.bare_text(nm).starts_with("uvm_"))
    })
}

/// The routine can report a failure: an assertion or an error-level
/// report in it, or in a routine it calls (three calls deep).
fn routine_can_fail(d: &Design, c: &Cst, n: NodeId, depth: u32, memo: &mut HashMap<(usize, u32), bool>) -> bool {
    if let Some(&v) = memo.get(&(c.index(), n.0)) {
        return v;
    }
    memo.insert((c.index(), n.0), false);
    let mut found = false;
    let mut callees = Vec::new();
    for x in c.subtree(n) {
        match c.kind(x) {
            AssertStmt | ConcurrentAssert => found = true,
            CallExpr => {
                if crate::support::class_index::report_level(c, x) >= crate::support::class_index::Level::Error {
                    found = true;
                }
                if let Some(r) = crate::support::slang_ast::callee_decl(c, x) {
                    callees.push(r);
                }
            }
            _ => {}
        }
        if found {
            break;
        }
    }
    if !found && depth < 3 {
        for r in callees {
            let r = out_of_block_body(c, r).unwrap_or(r);
            if c.kind(r) == MethodPrototype || routine_in_library(d, c, r) {
                continue;
            }
            if routine_can_fail(d, c, r, depth + 1, memo) {
                found = true;
                break;
            }
        }
    }
    memo.insert((c.index(), n.0), found);
    found
}

/// The body of an `extern` method: `function T cls::name(...)`.
fn out_of_block_body(c: &Cst, proto: NodeId) -> Option<NodeId> {
    if c.kind(proto) != MethodPrototype {
        return None;
    }
    let cls = std::iter::successors(c.parent(proto), |&a| c.parent(a)).find(|&a| c.kind(a) == ClassDecl)?;
    let cname = c.tree.bare_text(c.tree.child(cls, sv_syntax::SyntaxKind::Name)?);
    let name = crate::support::slang_ast::decl_name(c, proto);
    let want = format!("{cname}::{}", name.rsplit("::").next().unwrap_or(&name));
    c.nodes_of(&[FunctionDecl, TaskDecl]).into_iter().find(|&f| crate::support::slang_ast::decl_name(c, f) == want)
}

fn class_can_fail(d: &Design, ix: &Index, cls: &Class, memo: &mut HashMap<(usize, u32), bool>) -> bool {
    let mut cur = Some(cls);
    let mut guard = 0;
    while let Some(k) = cur {
        guard += 1;
        if guard > 32 {
            break;
        }
        // The library comparators report mismatches themselves.
        if k.name.contains("comparator") {
            return true;
        }
        if !crate::support::class_roles::is_library(d, k) {
            for m in k.methods.iter().filter(|m| !m.is_builtin && !m.is_pure) {
                let body = ix.body(m).or_else(|| {
                    // An extern method's body, out of the class.
                    let want = format!("{}::{}", k.name, m.name);
                    ix.csts.iter().find_map(|c| {
                        c.nodes_of(&[FunctionDecl, TaskDecl])
                            .into_iter()
                            .find(|&f| crate::support::slang_ast::decl_name(c, f) == want)
                            .map(|f| (c, f))
                    })
                });
                if let Some((c, body)) = body {
                    if routine_can_fail(d, c, body, 0, memo) {
                        return true;
                    }
                }
            }
            // A generic class has no methods in the model: its own routines
            // and out-of-block bodies from the syntax.
            if k.methods.is_empty() {
                if let Some((c, decl)) = ix.decl(k.loc).filter(|(c, n)| c.kind(*n) == ClassDecl) {
                    let prefix = format!("{}::", k.name);
                    let own = c.subtree(decl).into_iter().filter(|&f| matches!(c.kind(f), FunctionDecl | TaskDecl));
                    let outside = c
                        .nodes_of(&[FunctionDecl, TaskDecl])
                        .into_iter()
                        .filter(|&f| crate::support::slang_ast::decl_name(c, f).starts_with(&prefix));
                    let routines: Vec<NodeId> = own.chain(outside).collect();
                    if routines.into_iter().any(|f| routine_can_fail(d, c, f, 0, memo)) {
                        return true;
                    }
                }
            }
            for p in &k.properties {
                let pc = p.type_class.map(|i| d.class(i).name.to_lowercase());
                if pc.is_some_and(|n| n.contains("comparator")) {
                    return true;
                }
            }
        }
        cur = k.base.map(|b| d.class(b));
    }
    false
}

pub(crate) fn checker_cannot_fail(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    let classes: Vec<&Class> = crate::support::class_roles::user_classes(d).collect();
    // Base classes are judged through the classes that extend them.
    let extended: HashSet<String> = classes.iter().filter_map(|k| k.base.map(|b| d.class(b).name.clone())).collect();
    let mut memo = HashMap::new();
    for cls in classes {
        if cls.is_virtual || cls.is_interface || !looks_like_checker(d, cls) || extended.contains(&cls.name) {
            continue;
        }
        if class_can_fail(d, &ix, cls, &mut memo) {
            continue;
        }
        out.add(
            "UVM-TST-001",
            "CheckerCannotFail",
            Severity::Error,
            format!(
                "Checker '{}' never reports an error: it has no `uvm_error/`uvm_fatal, assertion or comparator. Every test using it \
passes regardless of what the DUT does.",
                cls.name
            ),
            cls.loc,
        );
    }
}
