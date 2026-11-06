//! Group A methodology gotchas: run_phase beside runtime phases, soft
//! constraints that shadow each other, virtual interfaces in checkers,
//! sequencers from config_db and config knob budgets.

use std::collections::HashMap;

use dv_model::{ClassId, Design};
use sv_syntax::NodeId;

use crate::g_meth_body::{bodies, calls_in, is_with_wrapper, Calls};
use crate::g_methodology::{role, role_name, user_classes, Ctx, Index, Role};
use crate::syntax::Cst;
use crate::Check::{Mixed, Model};
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "UVM-PHS-011", name: "RunPhaseWithRuntimePhases", check: Model(run_phase_with_runtime_phases) },
        Rule { id: "SV-RND-027", name: "SoftConstraintShadowing", check: Mixed(soft_constraint_shadowing) },
        Rule { id: "UVM-ROL-006", name: "VirtualInterfaceInChecker", check: Model(virtual_interface_in_checker) },
        Rule { id: "UVM-SEQ-016", name: "SequencerFromConfigDb", check: Mixed(sequencer_from_config_db) },
        Rule { id: "SV-MET-009", name: "ConfigKnobBudget", check: Model(config_knob_budget) },
    ]
}

fn run_phase_with_runtime_phases(d: &Design, out: &mut Sink) {
    for cls in user_classes(d) {
        if !crate::g_methodology::is_component_role(role(d, cls)) {
            continue;
        }
        // Subroutine members only: extern prototypes are not.
        let own = cls.methods.iter().filter(|m| !m.is_extern && !m.is_builtin);
        let mut run = None;
        let mut runtime = Vec::new();
        for m in own {
            if m.name == "run_phase" {
                run = Some(m);
            } else if m.name != "run_phase" && crate::is_task_phase(&m.name) {
                runtime.push(m.name.as_str());
            }
        }
        let Some(run) = run else { continue };
        if runtime.is_empty() {
            continue;
        }
        out.add(
            "UVM-PHS-011",
            "RunPhaseWithRuntimePhases",
            Severity::Warning,
            format!(
                "'{}' overrides run_phase and {}. run_phase runs alongside every runtime phase, so its stimulus or checks race \
the reset/configure/main schedule. Use either run_phase or the runtime phases.",
                cls.name,
                runtime.join(", ")
            ),
            run.loc,
        );
    }
}

fn virtual_interface_in_checker(d: &Design, out: &mut Sink) {
    for cls in user_classes(d) {
        let r = role(d, cls);
        if r != Role::Scoreboard && r != Role::Subscriber {
            continue;
        }
        for p in cls.properties.iter().filter(|p| p.is_virtual_interface) {
            out.add(
                "UVM-ROL-006",
                "VirtualInterfaceInChecker",
                Severity::Warning,
                format!(
                    "{} '{}' holds virtual interface '{}'. Have a monitor publish what the pins show (reset included) as \
transactions, and check those.",
                    role_name(r),
                    cls.name,
                    p.name
                ),
                p.loc,
            );
        }
    }
}

fn config_knob_budget(d: &Design, out: &mut Sink) {
    const LIMIT: usize = 12;
    for cls in user_classes(d) {
        if role(d, cls) != Role::Config {
            continue;
        }
        let knobs = cls.properties.iter().filter(|p| !p.is_static && !p.is_const).count();
        if knobs <= LIMIT {
            continue;
        }
        out.add(
            "SV-MET-009",
            "ConfigKnobBudget",
            Severity::Warning,
            format!(
                "Config '{}' has {knobs} knobs (budget {LIMIT}). Group related knobs into sub-config objects so each agent or feature \
reads only its own.",
                cls.name
            ),
            cls.loc,
        );
    }
}

fn sequencer_from_config_db(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for (cls, m, c, body) in bodies(d, &ix) {
        if role(d, cls) != Role::Sequence {
            continue;
        }
        let ctx = Ctx { d, cls, m: Some(m), td: Some(&ix.typedefs) };
        let calls = Calls::new(m);
        for call in calls_in(c, body) {
            if c.callee_name(call) != "get" {
                continue;
            }
            let owner = calls.owner(c, call);
            if owner != "uvm_config_db" && owner != "uvm_resource_db" {
                continue;
            }
            let args = c.call_args(call);
            let Some(t) = args.get(3).and_then(|a| c.arg_expr(*a)) else { continue };
            let Some(k) = ctx.class_of(c, t) else { continue };
            if role(d, k) != Role::Sequencer {
                continue;
            }
            out.add(
                "UVM-SEQ-016",
                "SequencerFromConfigDb",
                Severity::Warning,
                format!(
                    "Sequence '{}' looks up sequencer '{}' in uvm_config_db. Every sequence that does this repeats the plumbing and ties stimulus to hierarchy strings. Run it \
on a virtual sequencer (p_sequencer) or have the test hand it the handles.",
                    cls.name, k.name
                ),
                c.loc_of(call),
            );
        }
    }
}

// ---- SV-RND-027 ----

/// The expressions of `soft` constraints under a constraint block or item,
/// through lists, implications, conditionals and foreach.
fn soft_exprs(c: &Cst, n: NodeId, out: &mut Vec<NodeId>) {
    match c.kind(n) {
        ConstraintBlock => {
            for k in c.node_kids(n) {
                soft_exprs(c, k, out);
            }
        }
        ConstraintItem => {
            let kw = c.keyword(n).map(|t| c.text_of(t)).unwrap_or("");
            if kw == "soft" {
                if let Some(e) = c.tree.children(n).next() {
                    out.push(e);
                }
                return;
            }
            if kw == "if" || kw == "foreach" || c.tok(n, "->").is_some() {
                for k in c.node_kids(n) {
                    if matches!(c.kind(k), ConstraintItem | ConstraintBlock) {
                        soft_exprs(c, k, out);
                    }
                }
            }
        }
        _ => {}
    }
}

/// Rand properties named in an expression, in the order they appear.
fn rand_vars(ctx: &Ctx, c: &Cst, e: NodeId) -> Vec<(ClassId, String)> {
    let mut out: Vec<(ClassId, String)> = Vec::new();
    let mut push = |v: (ClassId, String)| {
        if !out.contains(&v) {
            out.push(v);
        }
    };
    for n in c.subtree(e) {
        let (owner, name) = match c.kind(n) {
            NameExpr if c.tree.children(n).next().is_none() => (ctx.cls, c.tree.bare_text(n)),
            MemberExpr => {
                let Some(obj) = c.tree.children(n).next() else { continue };
                let oc = if c.kind(obj) == NameExpr && c.tree.bare_text(obj) == "this" {
                    Some(ctx.cls)
                } else {
                    ctx.class_of(c, obj)
                };
                let Some(oc) = oc else { continue };
                (oc, c.last_text(n).to_string())
            }
            _ => continue,
        };
        let found = std::iter::once(owner)
            .chain(ctx.d.ancestors(owner.id))
            .find_map(|k| k.properties.iter().find(|p| p.name == name).map(|p| (k, p)));
        if let Some((k, p)) = found {
            if p.is_rand {
                push((k.id, name));
            }
        }
    }
    out
}

/// A class's constraint blocks written in it: (name, block).
fn constraint_blocks(c: &Cst, decl: NodeId) -> Vec<(String, NodeId)> {
    c.items(decl)
        .into_iter()
        .filter(|&x| c.kind(x) == ConstraintDecl)
        .filter_map(|x| {
            let name = c.tree.child(x, Name).map(|n| c.tree.bare_text(n)).unwrap_or_default();
            Some((name, c.tree.child(x, ConstraintBlock)?))
        })
        .collect()
}

fn soft_constraint_shadowing(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for cls in user_classes(d) {
        let Some((c, decl)) = ix.decl(cls.loc) else { continue };
        if c.kind(decl) != ClassDecl {
            continue;
        }
        // Soft targets of every user base class, nearest first.
        let mut inherited: HashMap<(ClassId, String), String> = HashMap::new();
        for b in d.ancestors(cls.id) {
            if crate::g_methodology::is_library(d, b) {
                break;
            }
            let Some((bc, bdecl)) = ix.decl(b.loc) else { continue };
            let bctx = Ctx { d, cls: b, m: None, td: Some(&ix.typedefs) };
            for (_, blk) in constraint_blocks(bc, bdecl) {
                let mut es = Vec::new();
                soft_exprs(bc, blk, &mut es);
                for e in es {
                    for v in rand_vars(&bctx, bc, e) {
                        inherited.entry(v).or_insert_with(|| b.name.clone());
                    }
                }
            }
        }
        let ctx = Ctx { d, cls, m: None, td: Some(&ix.typedefs) };
        let mut seen: HashMap<(ClassId, String), usize> = HashMap::new();
        let blocks = constraint_blocks(c, decl);
        for (bi, (_, blk)) in blocks.iter().enumerate() {
            let mut es = Vec::new();
            soft_exprs(c, *blk, &mut es);
            for e in es {
                for v in rand_vars(&ctx, c, e) {
                    let other = if let Some(b) = inherited.get(&v) {
                        format!("base class '{b}'")
                    } else if let Some(&s) = seen.get(&v).filter(|&&s| s != bi) {
                        format!("constraint '{}'", blocks[s].0)
                    } else {
                        String::new()
                    };
                    seen.entry(v.clone()).or_insert(bi);
                    if other.is_empty() {
                        continue;
                    }
                    out.add(
                        "SV-RND-027",
                        "SoftConstraintShadowing",
                        Severity::Warning,
                        format!(
                            "'{}' is also constrained softly in {other}. Soft constraints resolve by declaration order, so which default wins \
is hidden from the reader. Keep one soft default and override it with a hard constraint.",
                            v.1
                        ),
                        c.loc_of(e),
                    );
                    break;
                }
            }
        }
    }
    // `soft` inside randomize() with { } anywhere in user code.
    for c in csts {
        for n in c.nodes(CallExpr) {
            if !is_with_wrapper(c, n) {
                continue;
            }
            let Some(inner) = c.tree.children(n).next() else { continue };
            if c.callee_name(inner) != "randomize" {
                continue;
            }
            let Some(blk) = c.tree.child(n, WithClause).and_then(|w| c.tree.child(w, ConstraintBlock)) else { continue };
            if in_library(d, c, n) {
                continue;
            }
            let mut es = Vec::new();
            soft_exprs(c, blk, &mut es);
            for e in es {
                out.add(
                    "SV-RND-027",
                    "SoftConstraintShadowing",
                    Severity::Warning,
                    "'soft' in an inline randomize() with { } block. The caller's intent is a requirement here; a soft inline constraint quietly loses to any class \
constraint, so readers cannot tell what value they will get. Drop 'soft'."
                        .into(),
                    c.loc_of(e),
                );
            }
        }
    }
}

/// Inside a library class or uvm_pkg, or written in a library file.
fn in_library(d: &Design, c: &Cst, n: NodeId) -> bool {
    let f = d.file_name(c.loc_of(n));
    if f.contains("uvm_mock.svh") || f.contains("uvm_pkg.sv") {
        return true;
    }
    let mut p = c.parent(n);
    while let Some(x) = p {
        if matches!(c.kind(x), ClassDecl | PackageDecl) {
            let name = c.tree.child(x, Name).map(|k| c.tree.bare_text(k)).unwrap_or_default();
            if (c.kind(x) == ClassDecl && name.starts_with("uvm_")) || name == "uvm_pkg" {
                return true;
            }
        }
        p = c.parent(x);
    }
    false
}
