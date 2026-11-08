//! Group A config_db rules: config_db as a run-time channel, scattered
//! field names, shared config objects changed at run time, and get/set
//! pairing (MG-2).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use dv_model::{ClassId, Design, Loc, Method};
use sv_syntax::NodeId;

use crate::g_meth_body::{bodies, calls_in, factory_create_type, is_config_db_call, Calls};
use crate::g_meth_body2::{assignment_lhs, skip_selects};
use crate::g_methodology::{class_methods, is_component_role, key, role, user_classes, Ctx, Index, Typedefs};
use crate::syntax::Cst;
use crate::Check::Mixed;
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "UVM-CFG-007", name: "ConfigDbAsRuntimeChannel", check: Mixed(config_db_as_runtime_channel) },
        Rule { id: "UVM-CFG-008", name: "ScatteredConfigKeyLiteral", check: Mixed(scattered_config_key_literal) },
        Rule { id: "UVM-CFG-009", name: "SharedConfigMutatedAtRuntime", check: Mixed(shared_config_mutated_at_runtime) },
        Rule { id: "UVM-CFG-010", name: "ConfigDbGetWithoutSet", check: Mixed(config_db_get_without_set) },
        Rule { id: "UVM-CFG-011", name: "ConfigDbSetNeverGot", check: Mixed(config_db_set_never_got) },
    ]
}

fn is_run_time_phase(m: &str) -> bool {
    crate::is_task_phase(m)
}

fn is_build_time(m: &str) -> bool {
    matches!(m, "new" | "build_phase" | "connect_phase" | "end_of_elaboration_phase" | "start_of_simulation_phase")
}

fn config_db_access<'a>(calls: &Calls, c: &'a Cst, call: NodeId) -> Option<&'a str> {
    ["get", "set", "exists", "wait_modified"]
        .into_iter()
        .find(|m| is_config_db_call(calls, c, call, m))
        .map(|_| c.callee_name(call))
}

fn config_db_as_runtime_channel(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for (cls, m, c, body) in bodies(d, &ix) {
        let calls = Calls::new(m);
        let run_time = is_component_role(role(d, cls)) && is_run_time_phase(&m.name);
        for call in calls_in(c, body) {
            let Some(name) = config_db_access(&calls, c, call) else { continue };
            let mut in_loop = false;
            let mut p = c.parent(call);
            while let Some(x) = p {
                if x == body {
                    break;
                }
                if matches!(c.kind(x), LoopStmt | DoWhileStmt) {
                    in_loop = true;
                    break;
                }
                p = c.parent(x);
            }
            let in_task_loop = in_loop && m.is_task;
            if !run_time && !in_task_loop {
                continue;
            }
            out.add(
                "UVM-CFG-007",
                "ConfigDbAsRuntimeChannel",
                Severity::Warning,
                format!(
                    "'{}::{}' calls uvm_config_db::{name}() {}. config_db is for wiring the testbench while it is built; used as a \
run-time mailbox it is slow (a regex lookup per call) and hides who talks to whom. Read the value once in build_phase, and send run-time changes through a \
TLM port, an event or a method of the config object.",
                    cls.name,
                    m.name,
                    if run_time { "during the run-time phases" } else { "inside a loop of a task" }
                ),
                c.loc_of(call),
            );
        }
    }
}

/// `uvm_config_db#(T)::<method>(...)` written outside a class, where the
/// model has no calls: the scope names uvm_config_db (or a typedef of it).
fn syntactic_config_db_call(c: &Cst, td: &Typedefs, call: NodeId, method: &str) -> bool {
    if c.callee_name(call) != method {
        return false;
    }
    let Some(callee) = c.tree.children(call).next() else { return false };
    if c.kind(callee) != ScopedExpr {
        return false;
    }
    let Some(scope) = c.tree.children(callee).next() else { return false };
    let mut name = c.tree.bare_text(scope);
    for _ in 0..8 {
        let base = name.split('#').next().unwrap_or("").to_string();
        if base == "uvm_config_db" || base == "uvm_pkg::uvm_config_db" {
            return true;
        }
        match td.get(&base) {
            Some(t) => name = t.clone(),
            None => return false,
        }
    }
    false
}

/// Calls outside class methods: in free routines and module-level code.
fn outside_class_calls(c: &Cst) -> Vec<NodeId> {
    c.nodes(CallExpr)
        .into_iter()
        .filter(|&n| {
            let mut p = c.parent(n);
            while let Some(x) = p {
                if c.kind(x) == ClassDecl {
                    return false;
                }
                p = c.parent(x);
            }
            true
        })
        .collect()
}

fn scattered_config_key_literal(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    // key -> (file, offset, loc)
    let mut keys: BTreeMap<String, Vec<(String, u32, Loc)>> = BTreeMap::new();
    let mut seen = HashSet::new();
    let mut add = |c: &Cst, call: NodeId, keys: &mut BTreeMap<String, Vec<(String, u32, Loc)>>| {
        let args = c.call_args(call);
        let Some(k) = args.get(2).and_then(|a| c.arg_expr(*a)).and_then(|e| c.string_value(e)) else { return };
        let Some(first) = c.first_tok(call) else { return };
        let tok = &c.tree.tokens[first as usize];
        let loc = c.loc(first);
        if !seen.insert(key(loc)) || k.is_empty() || k == "*" {
            return;
        }
        // Where the call was written, after macro expansion.
        let (file, line) = loc.expansion.unwrap_or((loc.file, loc.line));
        let name = d.files().get(file as usize).cloned().unwrap_or_default();
        if name.contains("uvm_mock.svh") || name.rsplit(['/', '\\']).next().is_some_and(|b| b.starts_with("uvm_")) {
            return;
        }
        let _ = line;
        keys.entry(k).or_default().push((name, tok.origin.offset, loc));
    };
    for (_, m, c, body) in bodies(d, &ix) {
        let calls = Calls::new(m);
        for call in calls_in(c, body) {
            if is_config_db_call(&calls, c, call, "set") || is_config_db_call(&calls, c, call, "get") {
                add(c, call, &mut keys);
            }
        }
    }
    for c in csts {
        for call in outside_class_calls(c) {
            if syntactic_config_db_call(c, &ix.typedefs, call, "set") || syntactic_config_db_call(c, &ix.typedefs, call, "get") {
                add(c, call, &mut keys);
            }
        }
    }
    for (k, mut uses) in keys {
        let files: BTreeSet<String> = uses.iter().map(|u| u.0.clone()).collect();
        if files.len() < 2 {
            continue;
        }
        uses.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
        let mut others = String::new();
        let mut n = 0;
        for f in &files {
            if *f == uses[0].0 {
                continue;
            }
            if n == 3 {
                others.push_str(", ...");
                break;
            }
            n += 1;
            if !others.is_empty() {
                others.push_str(", ");
            }
            others.push_str(f.rsplit(['/', '\\']).next().unwrap_or(f));
        }
        out.add(
            "UVM-CFG-008",
            "ScatteredConfigKeyLiteral",
            Severity::Warning,
            format!(
                "The config_db field name \"{k}\" is typed as a literal {} times in {} files (also in {others}). If one \
spelling changes, get() quietly stops finding the value. Declare it once, for example as a localparam string in the agent's package, and use that name.",
                uses.len(),
                files.len()
            ),
            uses[0].2,
        );
    }
}

/// A variable or member, as `swe::referencedSymbol` names it: a local or
/// argument of one method, or a property of one class.
#[derive(Clone, PartialEq, Eq, Hash)]
enum Sym {
    Local((u32, u32, u32, Option<(u32, u32)>), String),
    Prop(ClassId, String),
}

fn name_sym(ctx: &Ctx, m: &Method, name: &str) -> Option<Sym> {
    if m.locals.iter().any(|l| l.name == name) || m.arguments.iter().any(|a| a.name == name) {
        return Some(Sym::Local(key(m.loc), name.to_string()));
    }
    std::iter::once(ctx.cls)
        .chain(ctx.d.ancestors(ctx.cls.id))
        .find(|k| k.properties.iter().any(|p| p.name == name))
        .map(|k| Sym::Prop(k.id, name.to_string()))
}

fn is_this(c: &Cst, n: NodeId) -> bool {
    c.kind(n) == NameExpr && matches!(c.tree.bare_text(n).as_str(), "this" | "super")
}

fn referenced_sym(ctx: &Ctx, m: &Method, c: &Cst, e: NodeId) -> Option<Sym> {
    let e = c.strip(e);
    match c.kind(e) {
        NameExpr if c.tree.children(e).next().is_none() => name_sym(ctx, m, &c.tree.bare_text(e)),
        MemberExpr => {
            let obj = c.tree.children(e).next()?;
            let member = c.last_text(e);
            if is_this(c, obj) {
                return name_sym(ctx, m, member);
            }
            let oc = ctx.class_of(c, obj)?;
            std::iter::once(oc)
                .chain(ctx.d.ancestors(oc.id))
                .find(|k| k.properties.iter().any(|p| p.name == member))
                .map(|k| Sym::Prop(k.id, member.to_string()))
        }
        _ => None,
    }
}

fn root_sym(ctx: &Ctx, m: &Method, c: &Cst, e: NodeId) -> Option<Sym> {
    let mut x = c.strip(e);
    loop {
        match c.kind(x) {
            SelectExpr => x = c.strip(c.tree.children(x).next()?),
            MemberExpr => {
                let obj = c.strip(c.tree.children(x).next()?);
                if is_this(c, obj) {
                    return name_sym(ctx, m, c.last_text(x));
                }
                x = obj;
            }
            NameExpr => return name_sym(ctx, m, &c.tree.bare_text(x)),
            _ => return None,
        }
    }
}

fn shared_config_mutated_at_runtime(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for cls in user_classes(d) {
        let methods: Vec<&Method> = class_methods(cls).collect();
        let (mut fetched, mut created) = (HashSet::new(), HashSet::new());
        for m in &methods {
            let Some((c, body)) = ix.body(m) else { continue };
            let ctx = Ctx { d, cls, m: Some(m), td: Some(&ix.typedefs) };
            let calls = Calls::new(m);
            for n in c.subtree(body) {
                if c.kind(n) == CallExpr && is_config_db_call(&calls, c, n, "get") {
                    let args = c.call_args(n);
                    if args.len() >= 4 {
                        if let Some(t) = c.arg_expr(args[3]) {
                            if ctx.class_of(c, t).is_some() {
                                if let Some(s) = referenced_sym(&ctx, m, c, t) {
                                    fetched.insert(s);
                                }
                            }
                        }
                    }
                }
                if let Some(lhs) = assignment_lhs(c, n) {
                    let Some(rhs) = c.tree.children(n).nth(1).map(|r| c.strip(r)) else { continue };
                    let creates = c.kind(rhs) == NewExpr
                        || (c.kind(rhs) == CallExpr && factory_create_type(&ctx, &calls, c, rhs).is_some())
                        || (c.kind(rhs) == CallExpr
                            && c.tree.children(rhs).next().is_some_and(|k| c.kind(k) == ScopedExpr && c.last_text(k) == "new"));
                    if creates {
                        if let Some(s) = referenced_sym(&ctx, m, c, skip_selects(c, lhs)) {
                            created.insert(s);
                        }
                    }
                }
            }
        }
        let shared: HashSet<Sym> = fetched.difference(&created).cloned().collect();
        if shared.is_empty() {
            continue;
        }
        let build_only = build_time_helpers(cls, &methods);
        for m in &methods {
            if is_build_time(&m.name) || build_only.contains(&m.name.as_str()) {
                continue;
            }
            let Some((c, body)) = ix.body(m) else { continue };
            let ctx = Ctx { d, cls, m: Some(m), td: Some(&ix.typedefs) };
            for n in c.subtree(body) {
                let Some(lhs) = assignment_lhs(c, n) else { continue };
                let l = skip_selects(c, lhs);
                if c.kind(l) != MemberExpr {
                    continue;
                }
                // The member must be a class property.
                let Some(obj) = c.tree.children(l).next() else { continue };
                let member = c.last_text(l);
                let owner = if is_this(c, obj) { Some(cls) } else { ctx.class_of(c, obj) };
                let Some(owner) = owner else { continue };
                if !std::iter::once(owner)
                    .chain(d.ancestors(owner.id))
                    .any(|k| k.properties.iter().any(|p| p.name == member))
                {
                    continue;
                }
                let Some(root) = root_sym(&ctx, m, c, l) else { continue };
                if !shared.contains(&root) {
                    continue;
                }
                out.add(
                    "UVM-CFG-009",
                    "SharedConfigMutatedAtRuntime",
                    Severity::Warning,
                    format!(
                        "'{}::{}' writes '{}' in a config object it got from uvm_config_db. Every component that read the same object sees the change at an unknown time. Let \
the owner that created the object change it, or copy it before changing it.",
                        cls.name,
                        m.name,
                        c.expr_text(l)
                    ),
                    c.loc_of(l),
                );
            }
        }
    }
}

/// Methods only called from new() and the build phases (directly or
/// through other such helpers).
fn build_time_helpers<'m>(cls: &dv_model::Class, methods: &[&'m Method]) -> HashSet<&'m str> {
    let own: HashSet<&str> = methods.iter().map(|m| m.name.as_str()).collect();
    let mut callers: HashMap<&str, HashSet<&str>> = HashMap::new();
    for m in methods {
        for k in &m.calls {
            if !k.is_system && k.callee_scope == cls.name && own.contains(k.name.as_str()) {
                callers.entry(k.name.as_str()).or_default().insert(m.name.as_str());
            }
        }
    }
    let mut out: HashSet<&str> = HashSet::new();
    let mut changed = true;
    while changed {
        changed = false;
        for m in methods {
            let n = m.name.as_str();
            if out.contains(n) || is_build_time(n) || !callers.contains_key(n) {
                continue;
            }
            if callers[n].iter().all(|c| is_build_time(c) || out.contains(c)) {
                out.insert(n);
                changed = true;
            }
        }
    }
    out
}

/// Field names with where each is used.
type FieldUses = Vec<(String, Loc)>;

fn field_uses(d: &Design, csts: &[Cst]) -> (FieldUses, FieldUses) {
    let ix = Index::new(csts);
    let (mut sets, mut gets) = (Vec::new(), Vec::new());
    for (_, m, c, body) in bodies(d, &ix) {
        let calls = Calls::new(m);
        for call in calls_in(c, body) {
            let is_set = is_config_db_call(&calls, c, call, "set");
            let is_get = is_config_db_call(&calls, c, call, "get");
            let args = c.call_args(call);
            if (!is_set && !is_get) || args.len() < 3 {
                continue;
            }
            let Some(field) = c.arg_expr(args[2]).and_then(|e| c.string_value(e)) else { continue };
            if field.is_empty() || field == "*" {
                continue;
            }
            if is_set { &mut sets } else { &mut gets }.push((field, c.loc_of(call)));
        }
    }
    (sets, gets)
}

fn config_db_get_without_set(d: &Design, csts: &[Cst], out: &mut Sink) {
    let (sets, gets) = field_uses(d, csts);
    if sets.is_empty() || gets.is_empty() {
        return;
    }
    let names: HashSet<&str> = sets.iter().map(|s| s.0.as_str()).collect();
    for (f, loc) in &gets {
        if names.contains(f.as_str()) {
            continue;
        }
        out.add(
            "UVM-CFG-010",
            "ConfigDbGetWithoutSet",
            Severity::Warning,
            format!(
                "uvm_config_db::get of '{f}' has no uvm_config_db::set of that field name in this run. The get returns false and the target stays at its default. Set the \
field from the test or env, with the same type parameter."
            ),
            *loc,
        );
    }
}

fn config_db_set_never_got(d: &Design, csts: &[Cst], out: &mut Sink) {
    let (sets, gets) = field_uses(d, csts);
    if sets.is_empty() || gets.is_empty() {
        return;
    }
    let names: HashSet<&str> = gets.iter().map(|s| s.0.as_str()).collect();
    for (f, loc) in &sets {
        if names.contains(f.as_str()) {
            continue;
        }
        out.add(
            "UVM-CFG-011",
            "ConfigDbSetNeverGot",
            Severity::Warning,
            format!(
                "uvm_config_db::set of '{f}' has no uvm_config_db::get of that field name in this run. The value is stored and never read. Remove the set, or get it where \
the component is built."
            ),
            *loc,
        );
    }
}
