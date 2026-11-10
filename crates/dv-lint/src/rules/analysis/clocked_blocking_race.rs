//! SV-RCE-002 (ClockedBlockingRace): Flags a variable written with a blocking
//! assignment in one edge-triggered always block and read in another block
//! triggered by the same edge of the same clock. Both run in the same time
//! step in an order IEEE 1800 leaves open (4.7), so the reader gets the old
//! or the new value depending on the simulator: a race. Write it with a
//! nonblocking assignment (<=).

use std::collections::{HashMap, HashSet};

use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

/// `posedge clk` / `negedge clk` of an always block with one edge event.
fn edge_of(c: &Cst, blk: NodeId) -> Option<(NodeId, String)> {
    let kw = c.keyword(blk).map(|t| c.text_of(t))?;
    if !matches!(kw, "always" | "always_ff") {
        return None;
    }
    let st = c.tree.children(blk).next()?;
    if c.kind(st) != TimedStmt {
        return None;
    }
    let tc = c.tree.child(st, TimingControl)?;
    let evs: Vec<NodeId> = c.tree.children(tc).collect();
    let [ev] = evs[..] else { return None };
    if c.kind(ev) != EventExpr {
        return None;
    }
    let edge = c.toks(ev).next().map(|t| c.text_of(t))?;
    if !matches!(edge, "posedge" | "negedge") {
        return None;
    }
    let sig = c.tree.children(ev).next()?;
    Some((st, format!("{edge} {}", c.trimmed(sig))))
}

fn walk(c: &Cst, n: NodeId, out: &mut Vec<NodeId>) {
    out.push(n);
    for k in c.tree.children(n) {
        walk(c, k, out);
    }
}

pub(crate) fn clocked_blocking_race(c: &Cst, s: &mut Sink) {
    for unit in c.nodes(ModuleDecl).into_iter().chain(c.nodes(InterfaceDecl)).chain(c.nodes(ProgramDecl)) {
        // Per block: its edge, blocking writes (name -> first token), reads, locals.
        let mut blocks = Vec::new();
        for blk in c.tree.children(unit).filter(|&k| c.kind(k) == ProceduralBlock) {
            let Some((body, edge)) = edge_of(c, blk) else { continue };
            let mut all = Vec::new();
            walk(c, body, &mut all);
            let mut locals = HashSet::new();
            let mut writes: HashMap<String, u32> = HashMap::new();
            let mut lhs = HashSet::new();
            for &n in &all {
                if c.kind(n) == DataDecl {
                    for d in c.tree.children(n).filter(|&d| c.kind(d) == Declarator) {
                        if let Some(nm) = c.tree.child(d, Name) {
                            locals.insert(c.tree.bare_text(nm));
                        }
                    }
                }
                if c.kind(n) == BinaryExpr && c.toks(n).next().map(|t| c.text_of(t)) == Some("=") {
                    let Some(target) = c.tree.children(n).next() else { continue };
                    if c.kind(target) == NameExpr {
                        lhs.insert(target);
                        let name = c.tree.bare_text(target);
                        if let Some(t) = c.first_tok(target) {
                            writes.entry(name).or_insert(t);
                        }
                    }
                }
            }
            let reads: HashSet<String> = all
                .iter()
                .filter(|&&n| c.kind(n) == NameExpr && !lhs.contains(&n))
                .map(|&n| c.tree.bare_text(n))
                .collect();
            for l in &locals {
                writes.remove(l);
            }
            blocks.push((blk, edge, writes, reads));
        }
        let mut seen = HashSet::new();
        for (i, (_, edge, writes, _)) in blocks.iter().enumerate() {
            let mut names: Vec<(&String, &u32)> = writes.iter().collect();
            names.sort_by_key(|(_, t)| **t);
            for (name, &tok) in names {
                let reader =
                    blocks.iter().enumerate().find(|(j, (_, e, _, r))| *j != i && e == edge && r.contains(name));
                let Some((_, (rblk, _, _, _))) = reader else { continue };
                if !seen.insert(name.clone()) {
                    continue;
                }
                let rline = c.keyword(*rblk).map(|t| c.loc(t).line).unwrap_or(0);
                s.add(
                    "SV-RCE-002",
                    "ClockedBlockingRace",
                    Severity::Warning,
                    format!(
                        "'{name}' is written with a blocking assignment in an always block on '{edge}' and read by another block on the same edge \
(line {rline}). Both run in the same time step in no fixed order (IEEE 1800 4.7), so the reader sees the old or the new value \
depending on the simulator. Use a nonblocking assignment (<=)."
                    ),
                    c.loc(tok),
                );
            }
        }
    }
}
