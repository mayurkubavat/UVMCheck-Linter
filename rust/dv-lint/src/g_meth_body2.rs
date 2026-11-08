//! Group A body rules, second batch: factory names as strings, item
//! contracts, writes into other components and hierarchical references
//! (MG-2, MG-4).

use std::collections::{HashMap, HashSet};

use dv_model::{Class, Design};
use sv_syntax::NodeId;

use crate::g_meth_body::{bodies, calls_in, Calls};
use crate::g_methodology::{class_methods, is_component_role, is_library, role, role_name, user_classes, Ctx, Index};
use crate::syntax::Cst;
use crate::Check::Mixed;
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "UVM-RUS-005", name: "OverrideByNameString", check: Mixed(override_by_name_string) },
        Rule { id: "UVM-RUS-006", name: "IncompleteItemContract", check: Mixed(incomplete_item_contract) },
        Rule { id: "UVM-CPL-003", name: "ForeignComponentStateWrite", check: Mixed(foreign_component_state_write) },
        Rule { id: "UVM-CPL-004", name: "HierarchicalReferenceInClass", check: Mixed(hierarchical_reference_in_class) },
    ]
}

/// Factory calls that take type names as strings, and which arguments
/// those are.
fn type_name_args(owner: &str, n: &str) -> &'static [usize] {
    match (owner, n) {
        ("uvm_factory", "set_type_override_by_name" | "set_inst_override_by_name") => &[0, 1],
        ("uvm_factory", "create_object_by_name" | "create_component_by_name") => &[0],
        ("uvm_component", "set_type_override") => &[0, 1],
        ("uvm_component", "set_inst_override") => &[1, 2],
        ("uvm_component", "create_object" | "create_component") => &[0],
        _ => &[],
    }
}

fn report_override_by_name(c: &Cst, call: NodeId, owner: &str, out: &mut Sink) {
    let n = c.callee_name(call);
    let idx = type_name_args(owner, n);
    if idx.is_empty() {
        return;
    }
    let args = c.call_args(call);
    let mut names = Vec::new();
    for &i in idx {
        let Some(v) = args.get(i).and_then(|a| c.arg_expr(*a)).and_then(|e| c.string_value(e)) else { return };
        names.push(format!("\"{v}\""));
    }
    out.add(
        "UVM-RUS-005",
        "OverrideByNameString",
        Severity::Warning,
        format!(
            "{n}() names types as strings ({}). A misspelt or renamed type is only noticed when the test runs, and often \
not even then. Use the typed form, such as base_t::type_id::set_type_override(derived_t::get_type()).",
            names.join(", ")
        ),
        c.loc_of(call),
    );
}

/// Subroutines outside classes, in user code (`swe::freeRoutines`).
pub(crate) fn free_routines(d: &Design, c: &Cst) -> Vec<NodeId> {
    c.nodes_of(&[FunctionDecl, TaskDecl])
        .into_iter()
        .filter(|&n| {
            let mut p = c.parent(n);
            while let Some(x) = p {
                if c.kind(x) == ClassDecl {
                    return false;
                }
                p = c.parent(x);
            }
            let f = d.file_name(c.loc_of(n));
            !f.contains("uvm_mock.svh") && !f.contains("uvm_pkg.sv") && c.tree.child(n, Body).is_some()
        })
        .collect()
}

/// Declared types of the names a free routine declares (ports and locals).
fn routine_names(c: &Cst, r: NodeId) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for n in c.subtree(r) {
        if !matches!(c.kind(n), Port | DataDecl) {
            continue;
        }
        let Some(ty) = c.tree.child(n, DataType) else { continue };
        for dcl in c.tree.children(n).filter(|&x| c.kind(x) == Declarator) {
            if let Some(name) = c.tree.child(dcl, Name) {
                out.entry(c.tree.bare_text(name)).or_insert_with(|| c.tree.bare_text(ty));
            }
        }
    }
    out
}

fn override_by_name_string(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for (_, m, c, body) in bodies(d, &ix) {
        let calls = Calls::new(m);
        for call in calls_in(c, body) {
            report_override_by_name(c, call, calls.owner(c, call), out);
        }
    }
    for c in csts {
        for r in free_routines(d, c) {
            let names = routine_names(c, r);
            let Some(body) = c.tree.child(r, Body) else { continue };
            for call in calls_in(c, body) {
                let Some(callee) = c.tree.children(call).next() else { continue };
                if c.kind(callee) != MemberExpr {
                    continue;
                }
                let Some(obj) = c.tree.children(callee).next() else { continue };
                if c.kind(obj) != NameExpr {
                    continue;
                }
                let Some(ty) = names.get(&c.tree.bare_text(obj)) else { continue };
                let Some(k) = crate::g_methodology::class_of_type_in(d, Some(&ix.typedefs), ty, None) else { continue };
                let n = c.callee_name(call);
                let owner = std::iter::once(k)
                    .chain(d.ancestors(k.id))
                    .find(|x| x.methods.iter().any(|mm| mm.name == n))
                    .map(|x| x.name.as_str())
                    .unwrap_or("");
                report_override_by_name(c, call, owner, out);
            }
        }
    }
}

/// The class's text as written, from `class` to `endclass`.
fn class_text<'t>(c: &Cst<'t>, n: NodeId) -> &'t str {
    let Some((a, b)) = c.tree.token_bounds(n) else { return "" };
    let (ta, tb) = (&c.tree.tokens[a as usize], &c.tree.tokens[b as usize]);
    if ta.from_macro || tb.from_macro || ta.origin.file != tb.origin.file {
        return "";
    }
    let src = &c.tree.sources[ta.origin.file as usize];
    let end = (tb.origin.offset as usize + tb.text.len()).min(src.len());
    src.get(ta.origin.offset as usize..end).unwrap_or("")
}

fn incomplete_item_contract(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for cls in user_classes(d) {
        if role(d, cls) != crate::g_methodology::Role::Item || cls.is_virtual {
            continue;
        }
        let (mut has_fields, mut macros) = (false, false);
        let mut methods = HashSet::new();
        let chain = std::iter::once(cls).chain(d.ancestors(cls.id));
        for k in chain {
            if is_library(d, k) {
                break;
            }
            has_fields |= k.properties.iter().any(|p| !p.is_static);
            if let Some((c, n)) = ix.decl(k.loc) {
                macros |= class_text(c, n).contains("`uvm_field_");
            }
            methods.extend(class_methods(k).map(|m| m.name.as_str()));
        }
        if !has_fields || macros {
            continue;
        }
        let mut missing = Vec::new();
        if !methods.contains("do_copy") {
            missing.push("do_copy()");
        }
        if !methods.contains("do_compare") {
            missing.push("do_compare()");
        }
        if !methods.contains("convert2string") && !methods.contains("do_print") {
            missing.push("convert2string() or do_print()");
        }
        if missing.is_empty() {
            continue;
        }
        let mut list = String::new();
        for (i, m) in missing.iter().enumerate() {
            list.push_str(if i == 0 {
                ""
            } else if i + 1 == missing.len() {
                " and "
            } else {
                ", "
            });
            list.push_str(m);
        }
        out.add(
            "UVM-RUS-006",
            "IncompleteItemContract",
            Severity::Warning,
            format!(
                "Sequence item '{}' has fields but no {list}. Without them copy(), clone() and compare() ignore the fields, so a scoreboard \
comparing two items always passes, and logs print nothing useful.",
                cls.name
            ),
            cls.loc,
        );
    }
}

/// Assignments' left sides (`=`, compound ops, nonblocking `<=`).
pub(crate) fn assignment_lhs(c: &Cst, n: NodeId) -> Option<NodeId> {
    if c.kind(n) != BinaryExpr {
        return None;
    }
    let op = c.text_of(c.toks(n).next()?);
    let assign = matches!(op, "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=" | "<<<=" | ">>>=")
        || (op == "<=" && c.parent(n).is_some_and(|p| c.kind(p) == ExprStmt));
    if !assign {
        return None;
    }
    c.tree.children(n).next()
}

pub(crate) fn skip_selects(c: &Cst, mut n: NodeId) -> NodeId {
    loop {
        n = c.strip(n);
        if c.kind(n) != SelectExpr {
            return n;
        }
        match c.tree.children(n).next() {
            Some(x) => n = x,
            None => return n,
        }
    }
}

fn is_this_or_super(c: &Cst, e: NodeId) -> bool {
    let x = skip_selects(c, e);
    c.kind(x) == NameExpr && matches!(c.tree.bare_text(x).as_str(), "this" | "super")
}

/// The class (or a base) whose property `name` is, if any.
fn property_owner<'d>(d: &'d Design, c: &'d Class, name: &str) -> Option<&'d Class> {
    std::iter::once(c)
        .chain(d.ancestors(c.id))
        .find(|k| k.properties.iter().any(|p| p.name == name))
}

/// A descendant reached from one of the class's own (or inherited)
/// properties: `agt`, `agt[0]`, `this.agt`, `agt.drv` ...
fn reached_through_own_child(ctx: &Ctx, c: &Cst, e: NodeId) -> bool {
    let mut x = skip_selects(c, e);
    while c.kind(x) == MemberExpr {
        let Some(v) = c.tree.children(x).next() else { return false };
        if is_this_or_super(c, v) {
            break;
        }
        x = skip_selects(c, v);
    }
    let name = match c.kind(x) {
        NameExpr if c.tree.children(x).next().is_none() => c.tree.bare_text(x),
        MemberExpr => c.last_text(x).to_string(),
        _ => return false,
    };
    if c.kind(x) == NameExpr {
        if let Some(m) = ctx.m {
            if m.locals.iter().any(|l| l.name == name) || m.arguments.iter().any(|a| a.name == name) {
                return false;
            }
        }
    }
    property_owner(ctx.d, ctx.cls, &name).is_some()
}

fn foreign_component_state_write(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for (cls, m, c, body) in bodies(d, &ix) {
        let ctx = Ctx { d, cls, m: Some(m), td: Some(&ix.typedefs) };
        let build_time = matches!(m.name.as_str(), "build_phase" | "connect_phase" | "end_of_elaboration_phase" | "new");
        for n in c.subtree(body) {
            let Some(lhs) = assignment_lhs(c, n) else { continue };
            let target = skip_selects(c, lhs);
            if c.kind(target) != MemberExpr {
                continue;
            }
            let Some(obj) = c.tree.children(target).next() else { continue };
            if is_this_or_super(c, obj) {
                continue;
            }
            let Some(oc) = ctx.class_of(c, obj) else { continue };
            if !is_component_role(role(d, oc)) || property_owner(d, oc, c.last_text(target)).is_none() {
                continue;
            }
            if build_time && reached_through_own_child(&ctx, c, obj) {
                continue;
            }
            let ot = c.expr_text(obj);
            if ot == "p_sequencer" || ot == "m_sequencer" {
                continue;
            }
            out.add(
                "UVM-CPL-003",
                "ForeignComponentStateWrite",
                Severity::Warning,
                format!(
                    "'{}::{}' writes '{}', state that belongs to {} '{}'. The other component can no longer change how it keeps that state, and the write \
is invisible from its code. Send the value through its config object, a TLM port or a method it provides.",
                    cls.name,
                    m.name,
                    c.expr_text(target),
                    role_name(role(d, oc)),
                    oc.name
                ),
                c.loc_of(target),
            );
        }
    }
}

fn hierarchical_reference_in_class(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    let mut units = HashSet::new();
    for c in csts {
        for n in c.nodes_of(&[ModuleDecl, InterfaceDecl, ProgramDecl]) {
            if let Some(name) = c.tree.child(n, Name) {
                units.insert(c.tree.bare_text(name));
            }
        }
    }
    for (cls, m, c, body) in bodies(d, &ix) {
        let ctx = Ctx { d, cls, m: Some(m), td: Some(&ix.typedefs) };
        for n in c.subtree(body) {
            if c.kind(n) != MemberExpr || c.parent(n).is_some_and(|p| c.kind(p) == MemberExpr) {
                continue;
            }
            if c.parent(n).is_some_and(|p| c.kind(p) == CallExpr && c.tree.children(p).next() == Some(n)) {
                continue;
            }
            let mut root = n;
            while c.kind(root) == MemberExpr {
                match c.tree.children(root).next() {
                    Some(x) => root = x,
                    None => break,
                }
            }
            if c.kind(root) != NameExpr || c.tree.children(root).next().is_some() {
                continue;
            }
            let name = c.tree.bare_text(root);
            let hier = name == "$root"
                || (units.contains(&name)
                    && ctx.class_of_name(&name).is_none()
                    && !m.locals.iter().any(|l| l.name == name)
                    && !m.arguments.iter().any(|a| a.name == name)
                    && property_owner(d, cls, &name).is_none());
            if !hier {
                continue;
            }
            out.add(
                "UVM-CPL-004",
                "HierarchicalReferenceInClass",
                Severity::Warning,
                format!(
                    "{}::{} reads '{}' through the module hierarchy. The class then only compiles and works in one testbench top; pass signals in through a \
virtual interface from config_db instead.",
                    cls.name,
                    m.name,
                    c.expr_text(n)
                ),
                c.loc_of(n),
            );
        }
    }
}
