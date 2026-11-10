//! SV-CNC-002 (CircularThreadWaitDeadlock): Detects circular lock acquisition
//! orders and wait-for dependency cycles across concurrent threads in
//! fork..join blocks, which cause guaranteed or probabilistic simulation
//! deadlocks.

use crate::support::procedural::{add_n, calls_in, items, start};
use crate::support::slang_ast::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Loc;
use svlang::syntax::SyntaxKind;
use SyntaxKind::*;

pub(crate) fn circular_wait(c: &Cst, s: &mut Sink) {
    for f in c.nodes(ParBlock) {
        let n = times(c, f);
        let mut seqs: Vec<Vec<(String, Loc)>> = Vec::new();
        for t in items(c, f) {
            let mut locks = Vec::new();
            for call in calls_in(c, t) {
                let Some((_, name, recv)) = call_parts(c, call) else { continue };
                if name != "get" {
                    continue;
                }
                // slang gives the receiver no syntax: read it off the call's text.
                let _ = recv;
                let text = slang_text(c, call);
                let sem = match text.find(".get") {
                    Some(dot) => {
                        let st = text[..dot].rfind(' ').map_or(0, |i| i + 1);
                        text[st..dot].to_string()
                    }
                    None => String::new(),
                };
                let sem = sem.trim_matches([' ', '\t', '\n', '\r']).to_string();
                if !sem.is_empty() {
                    if let Some(l) = start(c, call) {
                        locks.push((sem, l));
                    }
                }
            }
            if locks.len() >= 2 {
                seqs.push(locks);
            }
        }
        for t1 in 0..seqs.len() {
            for t2 in t1 + 1..seqs.len() {
                let (s1, s2) = (&seqs[t1], &seqs[t2]);
                for i in 0..s1.len() - 1 {
                    let (a, b) = (&s1[i].0, &s1[i + 1].0);
                    if a == b {
                        continue;
                    }
                    for j in 0..s2.len() - 1 {
                        if *a == s2[j + 1].0 && *b == s2[j].0 {
                            add_n(
                                s,
                                n,
                                "SV-CNC-002",
                                "CircularThreadWaitDeadlock",
                                Severity::Error,
                                format!(
                                    "Circular lock acquisition order detected between concurrent fork threads: \
                                     Thread 1 acquires '{a}' before '{b}', while Thread 2 acquires '{b}' before '{a}'. \
                                     This creates a circular wait dependency and guaranteed simulation deadlock. Enforce a global lock acquisition hierarchy."
                                ),
                                s2[j + 1].1,
                            );
                        }
                    }
                }
            }
        }
    }
}
