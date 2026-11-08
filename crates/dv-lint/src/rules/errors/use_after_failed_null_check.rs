//! SV-ERR-002 (UseAfterFailedNullCheck): Flags a handle that a null check, a
//! failed uvm_config_db::get() or a failed $cast() showed to be null,
//! dereferenced later on the same path because the failure branch did not stop
//! (SpotBugs NP_NULL_ON_SOME_PATH).

use std::collections::HashSet;
use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use crate::support::slang_ast::{call_parts, kids, user_routines};
use crate::support::procedural::{assign_lhs, call_args, items, start, PathHooks, Walker};
use crate::support::handles::{expr_decl, handle_type};

/// The declaration a handle-typed name or member access refers to.
fn tracked(c: &Cst, e: NodeId) -> Option<(u32, String)> {
    let mut e = e;
    while c.kind(e) == ParenExpr {
        e = *kids(c, e).first()?;
    }
    if !matches!(c.kind(e), NameExpr | MemberExpr) {
        return None;
    }
    let d = expr_decl(c, e)?;
    handle_type(c, d.dtype?, crate::support::slang_ast::enclosing_class(c, e), 0)?;
    let name = if c.kind(e) == MemberExpr { c.last_text(e).to_string() } else { c.tree.bare_text(e) };
    Some((d.node.0, name))
}

fn is_null(c: &Cst, e: NodeId) -> bool {
    let mut e = e;
    while c.kind(e) == ParenExpr {
        match kids(c, e).first() {
            Some(&x) => e = x,
            None => return false,
        }
    }
    matches!(c.kind(e), NameExpr | Literal) && c.tree.bare_text(e) == "null"
}

/// The handle a failing `$cast` or `uvm_config_db::get` leaves unset.
fn failed_call_target(c: &Cst, e: NodeId) -> Option<(u32, String)> {
    if c.kind(e) != CallExpr {
        return None;
    }
    let (_, n, _) = call_parts(c, e)?;
    let args = call_args(c, e);
    let target = if n == "$cast" {
        *args.first()?
    } else if n == "get" && args.len() == 4 && c.tree.bare_text(e).contains("uvm_config_db") {
        args[3]
    } else {
        return None;
    };
    tracked(c, target)
}

/// Handles known null on a path, with the line of the check.
#[derive(Clone)]
struct NullSet(Vec<(u32, String, u32)>);

impl PartialEq for NullSet {
    fn eq(&self, o: &Self) -> bool {
        self.0.len() == o.0.len() && self.0.iter().all(|a| o.0.iter().any(|b| b.0 == a.0))
    }
}

impl NullSet {
    fn add(&mut self, s: (u32, String), line: u32) {
        if !self.0.iter().any(|x| x.0 == s.0) {
            self.0.push((s.0, s.1, line));
        }
    }
    fn remove(&mut self, k: u32) {
        self.0.retain(|x| x.0 != k);
    }
}

struct NullHooks<'c, 't> {
    c: &'c Cst<'t>,
    reported: HashSet<NodeId>,
    found: Vec<(String, u32, NodeId)>,
}

impl NullHooks<'_, '_> {
    /// slang's AST visit of a statement: member accesses and method calls
    /// on a null handle are uses; an assignment to a handle kills it.
    fn scan(&self, n: NodeId, st: &NullSet, uses: &mut Vec<(u32, NodeId)>, kills: &mut Vec<u32>) {
        let c = self.c;
        if let Some(l) = assign_lhs(c, n) {
            if let Some((k, _)) = tracked(c, l) {
                kills.push(k);
                if let Some(&r) = kids(c, n).get(1) {
                    self.scan(r, st, uses, kills);
                }
                return;
            }
        }
        // Built-in methods (`randomize`) are system calls with no `this`.
        let system_callee = c.parent(n).is_some_and(|p| {
            c.kind(p) == CallExpr
                && kids(c, p).first() == Some(&n)
                && matches!(c.last_text(n), "randomize" | "srandom" | "get_randstate" | "set_randstate" | "rand_mode" | "constraint_mode")
        });
        if c.kind(n) == MemberExpr && !system_callee {
            if let Some(&b) = kids(c, n).first() {
                if let Some((k, _)) = tracked(c, b) {
                    if st.0.iter().any(|x| x.0 == k) {
                        uses.push((k, n));
                    }
                }
            }
        }
        for k in kids(c, n) {
            self.scan(k, st, uses, kills);
        }
    }

    fn line(&self, n: NodeId) -> u32 {
        start(self.c, n).map_or(0, |l| match l.expansion {
            Some((_, line)) => line,
            None => l.line,
        })
    }
}

impl<'c, 't> PathHooks<'c, 't> for NullHooks<'c, 't> {
    type St = NullSet;

    fn cst(&self) -> &'c Cst<'t> {
        self.c
    }

    fn leaf(&mut self, s: NodeId, st: &mut NullSet) {
        // slang's visit of a variable declaration statement skips its
        // initializer.
        if st.0.is_empty() || self.c.kind(s) == DataDecl {
            return;
        }
        let (mut uses, mut kills) = (Vec::new(), Vec::new());
        self.scan(s, st, &mut uses, &mut kills);
        for (k, at) in uses {
            let Some(i) = st.0.iter().position(|x| x.0 == k) else { continue };
            if self.reported.insert(at) {
                self.found.push((st.0[i].1.clone(), st.0[i].2, at));
            }
            st.0.remove(i);
        }
        for k in kills {
            st.remove(k);
        }
    }

    fn branch(&mut self, cond: NodeId, taken: bool, st: &mut NullSet) {
        let c = self.c;
        let mut e = cond;
        let mut negated = false;
        loop {
            match c.kind(e) {
                ParenExpr => e = match kids(c, e).first() {
                    Some(&x) => x,
                    None => return,
                },
                PrefixExpr if !negated && c.keyword(e).is_some_and(|k| c.text_of(k) == "!") => {
                    negated = true;
                    e = match kids(c, e).first() {
                        Some(&x) => x,
                        None => return,
                    };
                }
                _ => break,
            }
        }
        if c.kind(e) == BinaryExpr {
            let op = c.keyword(e).map_or("", |k| c.text_of(k));
            let eq = op == "==" || op == "===";
            let ne = op == "!=" || op == "!==";
            if !eq && !ne {
                return;
            }
            let k = kids(c, e);
            if k.len() != 2 {
                return;
            }
            let side = if is_null(c, k[1]) {
                k[0]
            } else if is_null(c, k[0]) {
                k[1]
            } else {
                return;
            };
            let Some(s) = tracked(c, side) else { return };
            if (eq == taken) != negated {
                let line = self.line(cond);
                st.add(s, line);
            } else {
                st.remove(s.0);
            }
            return;
        }
        if negated {
            if let Some(s) = failed_call_target(c, e) {
                if taken {
                    let line = self.line(cond);
                    st.add(s, line);
                } else {
                    st.remove(s.0);
                }
            }
        }
    }

    fn returned(&mut self, _s: NodeId, _st: &NullSet) {}
}

pub(crate) fn use_after_failed_null_check(c: &Cst, out: &mut Sink) {
    for (r, n) in user_routines(c) {
        let Some(body) = c.tree.child(r, Body) else { continue };
        let mut hooks = NullHooks { c, reported: HashSet::new(), found: Vec::new() };
        {
            let mut w = Walker { h: &mut hooks };
            let it = items(c, body);
            w.list(&it, vec![NullSet(Vec::new())]);
        }
        for (name, line, at) in hooks.found {
            let Some(loc) = start(c, at) else { continue };
            for _ in 0..n {
                out.add(
                    "SV-ERR-002",
                    "UseAfterFailedNullCheck",
                    Severity::Error,
                    format!(
                        "'{name}' is null on this path: the check at line {line} found it null but execution continued \
                         (`uvm_error does not stop the test). This dereference then crashes the simulator. Use `uvm_fatal, \
                         return, or assign a default before the use."
                    ),
                    loc,
                );
            }
        }
    }
}
