//! SV-SML-001 (ScratchpadClassProperty): Disallow declaring scratchpad
//! variables or loop induction counters as class properties.

use crate::support::class_index::{local_or_port, property_owner, referenced_name, routine_of, Classes};
use crate::support::statements::*;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::HashSet;
use sv_syntax::{NodeId, SyntaxKind};
use SyntaxKind::*;

pub(crate) fn scratchpad_property(c: &Cst, s: &mut Sink) {
    let units = Units::new(c);
    let classes = Classes::new(c);
    // slang visits members in order and a property already reported as a
    // loop counter is not reported again, so walk loops and properties in
    // source order.
    enum Ev {
        Loop(NodeId),
        Prop(usize, usize),
    }
    let mut events: Vec<(u32, Ev)> = Vec::new();
    for n in c.nodes(LoopStmt) {
        if stmt(c, n) == Some(St::For) {
            events.push((c.first_tok(n).unwrap_or(0), Ev::Loop(n)));
        }
    }
    for (i, cls) in classes.list.iter().enumerate() {
        for (pi, p) in cls.props.iter().enumerate() {
            events.push((p.tok, Ev::Prop(i, pi)));
        }
    }
    events.sort_by_key(|e| e.0);
    let mut flagged: HashSet<(usize, String)> = HashSet::new();
    for (_, ev) in events {
        match ev {
            Ev::Loop(n) => {
                let Some(ci) = classes.enclosing(c, n) else { continue };
                let Some(r) = routine_of(c, n) else { continue };
                for init in for_parts(c, n).init {
                    let Some((l, _, "=")) = assign_expr(c, init) else {
                        continue;
                    };
                    let l = strip_parens(c, l);
                    let name = match c.kind(l) {
                        NameExpr => match ident(c, l) {
                            Some(x) if !local_or_port(c, r, x) => x,
                            _ => continue,
                        },
                        MemberExpr if c.tree.bare_text(l).starts_with("this.") => match referenced_name(c, l) {
                            Some(x) => x,
                            None => continue,
                        },
                        _ => continue,
                    };
                    let Some(owner) = property_owner(&classes, ci, name) else {
                        continue;
                    };
                    report(
                        s,
                        units.times(c, n),
                        "SV-SML-001",
                        "ScratchpadClassProperty",
                        Severity::Warning,
                        format!(
                            "Loop induction variable '{name}' is declared as a class property rather than scoped locally to the loop or routine ('for (int {name} = 0; ...)'). \
                             Class-scoped loop counters introduce severe multi-threaded race conditions when routines execute concurrently in fork..join."
                        ),
                        start(c, init),
                    );
                    flagged.insert((owner, name.to_string()));
                }
            }
            Ev::Prop(i, pi) => {
                let cls = &classes.list[i];
                let p = &cls.props[pi];
                if !matches!(
                    p.name.as_str(),
                    "i" | "j" | "k" | "idx" | "index" | "temp" | "tmp" | "scratch" | "scratchpad" | "loop_cnt"
                ) {
                    continue;
                }
                if flagged.contains(&(i, p.name.clone())) || p.is_static || p.is_const {
                    continue;
                }
                report(
                    s,
                    units.times(c, cls.node),
                    "SV-SML-001",
                    "ScratchpadClassProperty",
                    Severity::Warning,
                    format!(
                        "Scratchpad variable '{}' is declared as a class property. Temporary variables and loop indices must be declared locally within the routine stack \
                         to prevent heap bloat and re-entrancy race conditions.",
                        p.name
                    ),
                    c.loc(p.tok),
                );
            }
        }
    }
}
