//! SV-RSC-001 (LockNotReleased): Flags semaphore get()/try_get() and sequencer
//! lock()/grab() that some path leaves without the matching
//! put()/unlock()/ungrab(), such as an early return (Sonar S2222).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use dv_model::Loc;
use sv_syntax::{NodeId, SyntaxKind};
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use SyntaxKind::*;
use crate::support::procedural::{PathHooks, Walker, add_n, calls_in, is_semaphore, items, routine_name, start};

#[derive(Clone, Copy, PartialEq, Eq)]
enum LockOp {
    None,
    Acquire,
    Release,
}

fn classify(c: &Cst, call: NodeId) -> (LockOp, String) {
    let Some((_, n, recv)) = call_parts(c, call) else { return (LockOp::None, String::new()) };
    if let Some(r) = recv {
        if is_semaphore(c, r) {
            // slang gives the receiver no syntax, so its text is empty.
            let _ = r;
            let key = String::new();
            return match n.as_str() {
                "get" => (LockOp::Acquire, key),
                "put" => (LockOp::Release, key),
                _ => (LockOp::None, key),
            };
        }
    }
    match n.as_str() {
        "lock" | "grab" => (LockOp::Acquire, n),
        "unlock" => (LockOp::Release, "lock".into()),
        "ungrab" => (LockOp::Release, "grab".into()),
        _ => (LockOp::None, String::new()),
    }
}

type Held = BTreeMap<String, i32>;

struct LockHooks<'c, 't> {
    c: &'c Cst<'t>,
    released: BTreeSet<String>,
    acquired_at: HashMap<String, Loc>,
    leaks: Vec<(String, Option<Loc>)>,
    reported: BTreeSet<String>,
}

impl<'c, 't> PathHooks<'c, 't> for LockHooks<'c, 't> {
    type St = Held;

    fn cst(&self) -> &'c Cst<'t> {
        self.c
    }

    fn leaf(&mut self, s: NodeId, st: &mut Held) {
        for call in calls_in(self.c, s) {
            self.apply(call, st);
        }
    }

    fn branch(&mut self, cond: NodeId, taken: bool, st: &mut Held) {
        self.try_get(cond, taken, st)
    }

    fn returned(&mut self, s: NodeId, st: &Held) {
        let at = start(self.c, s);
        self.leak(st, at);
    }
}

impl LockHooks<'_, '_> {
    fn apply(&mut self, call: NodeId, st: &mut Held) {
        let (op, key) = classify(self.c, call);
        match op {
            LockOp::Acquire => {
                *st.entry(key.clone()).or_insert(0) += 1;
                if let Some(l) = start(self.c, call) {
                    self.acquired_at.entry(key).or_insert(l);
                }
            }
            LockOp::Release => {
                if let Some(v) = st.get_mut(&key) {
                    *v -= 1;
                    if *v <= 0 {
                        st.remove(&key);
                    }
                }
            }
            LockOp::None => {}
        }
    }

    fn try_get(&mut self, cond: NodeId, taken: bool, st: &mut Held) {
        let c = self.c;
        let mut e = cond;
        let mut negated = false;
        if c.kind(e) == PrefixExpr && c.keyword(e).is_some_and(|k| c.text_of(k) == "!") {
            negated = true;
            let Some(&x) = kids(c, e).first() else { return };
            e = x;
        }
        while c.kind(e) == ParenExpr {
            let Some(&x) = kids(c, e).first() else { return };
            e = x;
        }
        if c.kind(e) != CallExpr {
            return;
        }
        let Some((_, name, Some(r))) = call_parts(c, e) else { return };
        if name != "try_get" {
            return;
        }
        if taken != negated {
            let _ = r;
            let key = String::new();
            *st.entry(key.clone()).or_insert(0) += 1;
            if let Some(l) = start(c, e) {
                self.acquired_at.entry(key).or_insert(l);
            }
        }
    }

    fn leak(&mut self, st: &Held, at: Option<Loc>) {
        for (key, &count) in st {
            if count <= 0 || !self.released.contains(key) || self.reported.contains(key) {
                continue;
            }
            self.reported.insert(key.clone());
            self.leaks.push((key.clone(), at));
        }
    }
}

pub(crate) fn lock_not_released(c: &Cst, s: &mut Sink) {
    for (r, n) in user_routines(c) {
        let mut hooks = LockHooks { c, released: BTreeSet::new(), acquired_at: HashMap::new(), leaks: Vec::new(), reported: BTreeSet::new() };
        let Some(body) = c.tree.child(r, Body) else { continue };
        for call in calls_in(c, body) {
            let (op, key) = classify(c, call);
            if op == LockOp::Release {
                hooks.released.insert(key);
            }
        }
        if hooks.released.is_empty() {
            continue;
        }
        let fall = {
            let mut w = Walker { h: &mut hooks };
            let it = items(c, body);
            w.list(&it, vec![Held::new()]).fall
        };
        let name_loc = c.tree.child(r, Name).and_then(|nm| {
            let toks = c.tree.token_range(nm);
            toks.last().map(|&t| c.loc(t))
        });
        for st in &fall {
            hooks.leak(st, None);
        }
        let rname = routine_name(c, r);
        for (key, at) in hooks.leaks.clone() {
            let acq = hooks.acquired_at.get(&key).map_or(0, |l| match l.expansion {
                Some((_, line)) => line,
                None => l.line,
            });
            let how = match at {
                None => format!("some path through '{rname}' ends without releasing it"),
                Some(_) => format!("this return leaves '{rname}' while still holding it"),
            };
            let Some(loc) = at.or(name_loc) else { continue };
            add_n(
                s,
                n,
                "SV-RSC-001",
                "LockNotReleased",
                Severity::Error,
                format!("'{key}' is acquired at line {acq} but {how}. The next requester then waits forever. Release it before every exit."),
                loc,
            );
        }
    }
}
