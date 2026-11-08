//! UVM-SCB-002 (ScoreboardLeftoverQueue): Scoreboard expected transaction
//! queues must be checked for drain completeness in check_phase or
//! report_phase.

use crate::support::uvm_classes::{is_scoreboard, Ix};
use crate::support::uvm_syntax::{routine_last_name, routine_text};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::Design;

/// The outermost unpacked dimension of a type string (`int$[$]`).
fn outer_dim(ty: &str) -> Option<&str> {
    let at = ty.find("$[")?;
    let rest = &ty[at + 2..];
    let mut depth = 0;
    for (i, ch) in rest.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' if depth == 0 => return Some(&rest[..i]),
            ']' => depth -= 1,
            _ => {}
        }
    }
    None
}

fn is_queue_or_assoc(ty: &str) -> bool {
    let Some(dim) = outer_dim(ty) else { return false };
    if dim == "$" || dim.starts_with("$:") {
        return true;
    }
    if dim.is_empty() {
        return false;
    }
    let fixed = dim.split(':').all(|p| {
        let p = p.trim();
        !p.is_empty() && p.trim_start_matches('-').chars().all(|c| c.is_ascii_digit())
    });
    !fixed
}

pub(crate) fn scoreboard_leftover_queue(d: &Design, csts: &[Cst], s: &mut Sink) {
    let ix = Ix::new(d, csts);
    for cls in ix.model_classes() {
        if !is_scoreboard(d, cls) {
            continue;
        }
        let props: Vec<_> = cls
            .properties
            .iter()
            .filter(|p| is_queue_or_assoc(&p.ty))
            .filter(|p| {
                let l = p.name.to_ascii_lowercase();
                l.contains("exp") || l.contains("pred") || l.contains("queue") || l.contains("_q")
            })
            .collect();
        if props.is_empty() {
            continue;
        }
        let mut check = String::new();
        let mut report = String::new();
        for at in ix.own_routines(cls.id) {
            let c = ix.cst(at);
            match routine_last_name(c, at.node).as_str() {
                "check_phase" => check = routine_text(c, at.node),
                "report_phase" => report = routine_text(c, at.node),
                _ => {}
            }
        }
        let raw = format!("{check}\n{report}");
        let mut clean = String::new();
        for line in raw.split('\n') {
            let line = line.find("//").map_or(line, |i| &line[..i]);
            clean.push_str(line);
            clean.push('\n');
        }
        // std::getline does not yield an empty last line.
        if raw.ends_with('\n') {
            clean.pop();
        }
        for p in props {
            let n = &p.name;
            let checked = [".size", ".num", ".empty", " == 0"].iter().any(|sfx| clean.contains(&format!("{n}{sfx}")));
            if checked {
                continue;
            }
            s.add(
                "UVM-SCB-002",
                "ScoreboardLeftoverQueue",
                Severity::Error,
                format!(
                    "Scoreboard class '{}' declares expected transaction container '{n}', but neither 'check_phase' nor 'report_phase' verifies that the container is empty \
                     at the end of simulation. If the DUT drops packets, simulation will complete with 0 errors. \
                     Add 'if ({n}.size() != 0) `uvm_error(\"DROPPED_PKT\", ...)' in check_phase().",
                    cls.name
                ),
                p.loc,
            );
        }
    }
}
