//! Deprecated system functions and UVM APIs (builder slice N6.3, group F),
//! ported from `src/rules/deprecated/`. The C++ rules visit slang's AST, so
//! a call is reported once per elaborated copy of the code around it: per
//! instance of its module and per specialization of its class (see
//! [`copies`]). Messages are verbatim.

use std::collections::{HashMap, HashSet};

use sv_syntax::NodeId;

use crate::syntax::Cst;
use crate::Check::Syntax;
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-DEP-004", name: "RandomSystemFunction", check: Syntax(random_system_function) },
        Rule { id: "UVM-DEP-001", name: "UvmDeprecatedConfigCalls", check: Syntax(deprecated_config_calls) },
        Rule { id: "UVM-DEP-002", name: "UvmDeprecatedReportServer", check: Syntax(deprecated_report_server) },
        Rule { id: "UVM-DEP-003", name: "UvmDeprecatedPrinterKnobs", check: Syntax(deprecated_printer_knobs) },
        Rule { id: "UVM-DEP-004", name: "UvmDeprecatedTestDone", check: Syntax(deprecated_test_done) },
    ]
}

// ---- elaborated copies ----

fn decl_name(c: &Cst, n: NodeId) -> String {
    c.tree.child(n, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default()
}

/// How many times slang's AST holds the code at `n`: the instances of its
/// module (a module nothing instantiates is a top, once) times, for a
/// parameterized class, its default specialization plus each other
/// parameter list it is used with.
pub(crate) fn copies(c: &Cst, n: NodeId) -> usize {
    let mut k = 1;
    let mut up = c.parent(n);
    while let Some(p) = up {
        match c.kind(p) {
            ModuleDecl | InterfaceDecl | ProgramDecl => k *= module_instances(c, &decl_name(c, p)),
            ClassDecl if c.tree.child(p, Header).and_then(|h| c.tree.child(h, ParamList)).is_some() => {
                k *= 1 + specializations(c, &decl_name(c, p));
            }
            _ => {}
        }
        up = c.parent(p);
    }
    k
}

/// Instances of module `name` in the tree, through the instances of the
/// modules that instantiate it; 1 for a top.
pub(crate) fn module_instances(c: &Cst, name: &str) -> usize {
    fn count(c: &Cst, name: &str, seen: &mut HashSet<String>) -> usize {
        if !seen.insert(name.to_string()) {
            return 1;
        }
        let mut total = 0;
        let mut any = false;
        for inst in c.nodes(Instantiation) {
            let Some(ty) = c.tree.child(inst, DataType) else { continue };
            if c.first_tok(ty).map(|t| c.text_of(t)) != Some(name) {
                continue;
            }
            any = true;
            let here = c.tree.children(inst).filter(|&x| c.kind(x) == Instance).count();
            let mut up = c.parent(inst);
            let mut outer = 1;
            while let Some(p) = up {
                if matches!(c.kind(p), ModuleDecl | InterfaceDecl | ProgramDecl) {
                    outer = count(c, &decl_name(c, p), seen);
                    break;
                }
                up = c.parent(p);
            }
            total += here * outer;
        }
        seen.remove(name);
        if any { total } else { 1 }
    }
    count(c, name, &mut HashSet::new())
}

/// Distinct parameter lists class `name` is written with (`name #(...)`).
fn specializations(c: &Cst, name: &str) -> usize {
    let mut lists = HashSet::new();
    for ty in c.nodes(DataType) {
        if c.first_tok(ty).map(|t| c.text_of(t)) != Some(name) {
            continue;
        }
        let Some(pl) = c.tree.child(ty, ParamList) else { continue };
        let text = c.tree.bare_text(pl);
        if text != "#()" {
            lists.insert(text);
        }
    }
    lists.len()
}

// ---- calls ----

/// slang's `CallExpression`s: calls with an argument list, and names of
/// system functions or known routines used without one. Each comes with
/// its subroutine name and first token (the start of the call).
fn calls(c: &Cst) -> Vec<(NodeId, String, u32)> {
    let mut out = Vec::new();
    for n in c.nodes(CallExpr) {
        let Some(callee) = c.tree.children(n).next() else { continue };
        let Some(first) = c.first_tok(n) else { continue };
        out.push((n, c.last_text(callee).to_string(), first));
    }
    for n in c.nodes(NameExpr).into_iter().chain(c.nodes(ScopedExpr)).chain(c.nodes(MemberExpr)) {
        let callee_of_call = c
            .parent(n)
            .is_some_and(|p| matches!(c.kind(p), CallExpr | ScopedExpr | MemberExpr) && c.tree.children(p).next() == Some(n));
        if callee_of_call {
            continue;
        }
        let Some(first) = c.first_tok(n) else { continue };
        out.push((n, c.last_text(n).to_string(), first));
    }
    out
}

/// Names of the routines the tree declares.
fn routine_names(c: &Cst) -> HashSet<String> {
    let mut out = HashSet::new();
    for n in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)).chain(c.nodes(MethodPrototype)) {
        let name = decl_name(c, n);
        out.insert(name.rsplit("::").next().unwrap_or("").to_string());
    }
    out
}

fn random_system_function(c: &Cst, s: &mut Sink) {
    for (n, name, first) in calls(c) {
        if name != "$random" {
            continue;
        }
        for _ in 0..copies(c, n) {
            s.add(
                "SV-DEP-004",
                "RandomSystemFunction",
                Severity::Warning,
                "Verilog system function '$random' is superseded in IEEE 1800 § 20.15. \
                 '$random' generates signed 32-bit integers that evaluate to negative values when used with modulo, \
                 and shares a single global LCG seed that corrupts simulation random stability. \
                 Replace with '$urandom' or '$urandom_range(min, max)'."
                    .into(),
                c.loc(first),
            );
        }
    }
}

const CONFIG_CALLS: [&str; 6] = [
    "set_config_int",
    "set_config_string",
    "set_config_object",
    "get_config_int",
    "get_config_string",
    "get_config_object",
];

/// Calls of declared routines named `want`, with their call text.
fn routine_calls(c: &Cst, want: &dyn Fn(&str) -> bool) -> Vec<(NodeId, String, u32)> {
    let declared = routine_names(c);
    calls(c)
        .into_iter()
        .filter(|(_, name, _)| want(name) && declared.contains(name))
        .collect()
}

fn deprecated_config_calls(c: &Cst, s: &mut Sink) {
    for (n, name, first) in routine_calls(c, &|x| CONFIG_CALLS.contains(&x)) {
        for _ in 0..copies(c, n) {
            s.add(
                "UVM-DEP-001",
                "UvmDeprecatedConfigCalls",
                Severity::Error,
                format!(
                    "Legacy configuration method '{name}' is formally deprecated in IEEE 1800.2 (Annex H). \
                     These string-based methods lack type safety and break under +define+UVM_NO_DEPRECATED. \
                     Replace with modern, type-safe 'uvm_config_db#(T)::set(this, ...)' and 'uvm_config_db#(T)::get(this, ...)'."
                ),
                c.loc(first),
            );
        }
    }
}

fn deprecated_report_server(c: &Cst, s: &mut Sink) {
    for (n, _, first) in routine_calls(c, &|x| x == "get_server") {
        if !c.text(n).contains("uvm_report_server") {
            continue;
        }
        for _ in 0..copies(c, n) {
            s.add(
                "UVM-DEP-002",
                "UvmDeprecatedReportServer",
                Severity::Warning,
                "Direct access via 'uvm_report_server::get_server()' is deprecated in IEEE 1800.2 § F.4.2 and Annex H. \
                 UVM report servers are now context-managed through uvm_coreservice_t. \
                 Replace with: 'uvm_coreservice_t cs = uvm_coreservice_t::get(); server = cs.get_report_server();'."
                    .into(),
                c.loc(first),
            );
        }
    }
}

fn deprecated_test_done(c: &Cst, s: &mut Sink) {
    for (n, name, first) in routine_calls(c, &|x| x == "raise_objection" || x == "drop_objection") {
        if !c.text(n).contains("uvm_test_done") {
            continue;
        }
        for _ in 0..copies(c, n) {
            s.add(
                "UVM-DEP-004",
                "UvmDeprecatedTestDone",
                Severity::Error,
                format!(
                    "Legacy OVM objection handle 'uvm_test_done' is obsolete in IEEE 1800.2 Annex H. \
                     Objections must be attached directly to the active phase handle ('phase.{name}(this)')."
                ),
                c.loc(first),
            );
        }
    }
}

/// The declared type of variable `name` as seen from `at`: the closest
/// declaration in an enclosing scope (data declarations and ports).
fn declared_type(c: &Cst, at: NodeId, name: &str, decls: &HashMap<String, Vec<(NodeId, String)>>) -> Option<String> {
    let cands = decls.get(name)?;
    let mut up = c.parent(at);
    while let Some(p) = up {
        // A declaration whose scope (parent of its declaration) is `p`.
        for (scope, ty) in cands {
            if *scope == p {
                return Some(ty.clone());
            }
        }
        up = c.parent(p);
    }
    None
}

fn deprecated_printer_knobs(c: &Cst, s: &mut Sink) {
    // Declared names by the scope that holds the declaration.
    let mut decls: HashMap<String, Vec<(NodeId, String)>> = HashMap::new();
    for d in c.nodes(DataDecl).into_iter().chain(c.nodes(Port)) {
        let Some(ty) = c.tree.child(d, DataType) else { continue };
        let ty = c.tree.bare_text(ty);
        let mut scope = c.parent(d);
        // Statements in a block or body: the block is the scope.
        while let Some(p) = scope {
            if matches!(c.kind(p), PortList | Header) {
                scope = c.parent(p);
                continue;
            }
            break;
        }
        let Some(scope) = scope else { continue };
        for dd in c.tree.children(d).filter(|&x| c.kind(x) == Declarator) {
            let Some(nm) = c.tree.child(dd, Name) else { continue };
            decls.entry(c.tree.bare_text(nm)).or_default().push((scope, ty.clone()));
        }
    }
    for n in c.nodes(MemberExpr) {
        if c.last_text(n) != "knobs" {
            continue;
        }
        let Some(recv) = c.tree.children(n).next() else { continue };
        if c.kind(recv) != NameExpr {
            continue;
        }
        let Some(ty) = declared_type(c, n, &c.tree.bare_text(recv), &decls) else { continue };
        if !ty.contains("printer") {
            continue;
        }
        let Some(first) = c.first_tok(n) else { continue };
        for _ in 0..copies(c, n) {
            s.add(
                "UVM-DEP-003",
                "UvmDeprecatedPrinterKnobs",
                Severity::Warning,
                "Direct access to internal 'printer.knobs' is deprecated in IEEE 1800.2 § 5.5.2. \
                 Use official policy accessor methods on uvm_printer (e.g. 'printer.set_max_depth()', 'printer.set_radix()')."
                    .into(),
                c.loc(first),
            );
        }
    }
}
