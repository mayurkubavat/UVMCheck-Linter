//! Group E (builder slice N6.3): the correctness checks of
//! `src/rules/correctness/` that read the syntax alone. See
//! [`crate::g_random`].

use std::collections::{HashMap, HashSet};

use sv_syntax::NodeId;

use crate::g_random::{call_name, class_decls, class_items, decl_name, declarators, kids, op, qualifiers, subtree,
    user_code};
use crate::g_random_oop::{is_number_literal, strip_conv, unparen};
use crate::g_random_sema::ast_ok;
use crate::syntax::Cst;
use crate::Check::Syntax;
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-BUG-003", name: "SelfAssignment", check: Syntax(self_assignment) },
        Rule { id: "SV-BUG-002", name: "RepeatedElseIfCondition", check: Syntax(repeated_else_if) },
        Rule { id: "SV-OOP-006", name: "ClassShallowCopyAliasing", check: Syntax(shallow_copy) },
        Rule { id: "SV-TST-001", name: "ConstantReseed", check: Syntax(constant_reseed) },
    ]
}

// ---- SV-BUG-003 ----

/// `x = x;` (blocking, plain `=`) where x is a name or a member access.
fn self_assignment(c: &Cst, s: &mut Sink) {
    for n in c.nodes(BinaryExpr) {
        if op(c, n) != "=" || !user_code(c, n) {
            continue;
        }
        let k = kids(c, n);
        let (Some(&l), Some(&r)) = (k.first(), k.get(1)) else { continue };
        if !matches!(c.kind(l), NameExpr | MemberExpr) {
            continue;
        }
        let (r, _) = strip_conv(c, r);
        let name = c.tree.bare_text(l);
        // slang binds `this.x` and `super.x` to the member itself.
        fn member(t: &str) -> &str {
            let mut t = t;
            while let Some(r) = t.strip_prefix("this.").or_else(|| t.strip_prefix("super.")) {
                t = r;
            }
            t
        }
        let rname = c.tree.bare_text(r);
        // A plain name next to `this.x` is the member only when no formal
        // or local of the routine takes the name.
        let plain_is_member = |t: &str| member(t) != t || !routine_declares(c, n, t);
        if name.is_empty()
            || member(&name) != member(&rname)
            || (name != rname && !(plain_is_member(&name) && plain_is_member(&rname)))
            || !matches!(c.kind(r), NameExpr | MemberExpr)
        {
            continue;
        }
        if !ast_ok(c, n) {
            continue;
        }
        let mut hint = String::new();
        if c.kind(l) == NameExpr && is_formal(c, n, &name) {
            hint = format!(
                " The argument '{name}' hides the class property of the same name; write 'this.{name} = {name};'."
            );
        }
        let Some(at) = c.first_tok(n) else { continue };
        s.add(
            "SV-BUG-003",
            "SelfAssignment",
            Severity::Warning,
            format!("'{name}' is assigned to itself, so the statement has no effect.{hint}"),
            c.loc(at),
        );
    }
}

/// `name` at `n` names a formal argument of the routine around it (and no
/// local of its body).
/// The routine around `n` declares `name` as a formal or a local.
fn routine_declares(c: &Cst, n: NodeId, name: &str) -> bool {
    let mut x = n;
    while let Some(p) = c.parent(x) {
        if matches!(c.kind(p), FunctionDecl | TaskDecl) {
            let declares = |kind, holder: Option<NodeId>| {
                holder.is_some_and(|h| {
                    c.tree
                        .children(h)
                        .filter(|&d| c.kind(d) == kind)
                        .any(|d| declarators(c, d).iter().any(|(_, v, _)| v == name))
                })
            };
            return declares(DataDecl, c.tree.child(p, Body)) || declares(Port, c.tree.child(p, PortList));
        }
        x = p;
    }
    false
}

fn is_formal(c: &Cst, n: NodeId, name: &str) -> bool {
    let mut x = n;
    while let Some(p) = c.parent(x) {
        if matches!(c.kind(p), FunctionDecl | TaskDecl) {
            let declares = |kind, holder: Option<NodeId>| {
                holder.is_some_and(|h| {
                    c.tree
                        .children(h)
                        .filter(|&d| c.kind(d) == kind)
                        .any(|d| declarators(c, d).iter().any(|(_, v, _)| v == name))
                })
            };
            if declares(DataDecl, c.tree.child(p, Body)) {
                return false;
            }
            return declares(Port, c.tree.child(p, PortList));
        }
        x = p;
    }
    false
}

/// Tokens joined, any trivia between two of them read as one space
/// (swe::exprText).
pub(crate) fn expr_text(c: &Cst, n: NodeId) -> String {
    let mut out = String::new();
    for (i, t) in c.tree.token_range(n).into_iter().enumerate() {
        let tok = &c.tree.tokens[t as usize];
        if i > 0 && (tok.space_before || !c.tree.trivia(t).is_empty()) {
            out.push(' ');
        }
        out.push_str(&tok.text);
    }
    out
}

// ---- SV-BUG-002 ----

/// The condition of an if statement when it is one plain expression.
fn if_condition(c: &Cst, n: NodeId) -> Option<NodeId> {
    let e = c.tree.children(n).find(|&k| crate::g_random_sema::is_expr(c.kind(k)))?;
    if c.kind(unparen(c, e)) == MatchExpr || c.toks(e).any(|t| c.text_of(t) == "&&&") {
        return None;
    }
    Some(e)
}

fn else_if(c: &Cst, n: NodeId) -> Option<NodeId> {
    let e = c.tree.child(n, ElseClause)?;
    c.tree.children(e).next().filter(|&k| c.kind(k) == IfStmt)
}

fn repeated_else_if(c: &Cst, s: &mut Sink) {
    for head in c.nodes(IfStmt) {
        if c.parent(head).is_some_and(|p| c.kind(p) == ElseClause) || !user_code(c, head) {
            continue;
        }
        let mut conds = Vec::new();
        let mut cur = Some(head);
        while let Some(n) = cur {
            if let Some(e) = if_condition(c, n) {
                conds.push(e);
            }
            cur = else_if(c, n);
        }
        let keys: Vec<String> = conds.iter().map(|&e| c.tree.bare_text(unparen(c, e))).collect();
        for i in 1..conds.len() {
            let text = expr_text(c, conds[i]);
            if ["$urandom", "$random", "randomize", "try_", "++", "--"].iter().any(|v| text.contains(v)) {
                continue;
            }
            let Some(j) = (0..i).find(|&j| keys[j] == keys[i]) else { continue };
            if !ast_ok(c, conds[i]) {
                continue;
            }
            let (Some(at), Some(first)) = (c.first_tok(unparen(c, conds[i])), c.first_tok(unparen(c, conds[j]))) else {
                continue;
            };
            s.add(
                "SV-BUG-002",
                "RepeatedElseIfCondition",
                Severity::Warning,
                format!(
                    "Condition '{text}' already appears at line {} in the same if/else-if chain, so this branch can \
                     never run. It usually hides a case that was meant to be handled here.",
                    c.loc(first).line
                ),
                c.loc(at),
            );
        }
    }
}

// ---- SV-TST-001 ----

/// uvm base classes swe::classRole knows; the first one up the chain
/// decides the role.
const UVM_ROLES: &[&str] = &[
    "uvm_driver", "uvm_push_driver", "uvm_monitor", "uvm_push_sequencer", "uvm_agent", "uvm_scoreboard",
    "uvm_subscriber", "uvm_env", "uvm_test", "uvm_mem", "uvm_vreg", "uvm_report_catcher", "uvm_sequence",
    "uvm_sequence_base", "uvm_reg_sequence", "uvm_sequence_item", "uvm_transaction", "uvm_component", "uvm_root",
    "uvm_object", "uvm_void",
];

fn known_role(n: &str) -> bool {
    UVM_ROLES.contains(&n)
        || n.starts_with("uvm_sequencer")
        || n.starts_with("uvm_reg")
        || n.starts_with("uvm_callback")
        || n.starts_with("uvm_tlm")
}

/// The name of the class a class extends, without parameters.
pub(crate) fn base_name(c: &Cst, cls: NodeId) -> Option<String> {
    let ext = c.tree.child(cls, Header).and_then(|h| c.tree.child(h, Extends))?;
    let base = c.tree.children(ext).next()?;
    Some(
        c.tree
            .children(base)
            .find(|&k| c.kind(k) == ParamList)
            .and_then(|p| c.first_tok(p))
            .map_or_else(|| c.last_text(base).to_string(), |p| c.text_of(p - 1).to_string()),
    )
}

fn is_test(c: &Cst, cls: NodeId, by_name: &HashMap<String, NodeId>) -> bool {
    let mut cur = cls;
    for _ in 0..64 {
        let Some(b) = base_name(c, cur) else { return false };
        if b.starts_with("uvm_") && known_role(&b) {
            return b == "uvm_test";
        }
        match by_name.get(&b) {
            Some(&k) => cur = k,
            None => return false,
        }
    }
    false
}

/// A value swe::isConstantExpr accepts: a literal, a parameter, or an
/// operator over those.
fn is_constant(c: &Cst, e: NodeId, params: &HashSet<String>) -> bool {
    let e = unparen(c, strip_conv(c, e).0);
    match c.kind(e) {
        Literal => is_number_literal(c, e) || c.trimmed(e).starts_with('\''),
        NameExpr => params.contains(&c.tree.bare_text(e)),
        BinaryExpr => kids(c, e).iter().all(|&k| is_constant(c, k, params)),
        _ => false,
    }
}

fn constant_reseed(c: &Cst, s: &mut Sink) {
    let classes: Vec<(NodeId, String)> = class_decls(c)
        .into_iter()
        .filter(|&k| user_code(c, k) && !c.tree.child(k, Header).is_some_and(|h| c.tok(h, "::").is_some()))
        .filter_map(|k| Some((k, decl_name(c, k)?.0)))
        .collect();
    let by_name: HashMap<String, NodeId> = classes.iter().map(|(k, n)| (n.clone(), *k)).collect();
    let mut params = HashSet::new();
    for d in c.nodes(ParamDecl) {
        for (_, v, _) in declarators(c, d) {
            params.insert(v);
        }
    }
    // Out-of-block bodies: class name -> routines.
    let mut outside: HashMap<String, Vec<NodeId>> = HashMap::new();
    for f in c.tree.children(c.tree.root).flat_map(|x| std::iter::once(x).chain(c.tree.children(x))) {
        if !matches!(c.kind(f), FunctionDecl | TaskDecl) {
            continue;
        }
        let Some(nm) = c.tree.child(f, Name) else { continue };
        let toks: Vec<u32> = c.tree.token_range(nm);
        if toks.len() == 3 && c.text_of(toks[1]) == "::" {
            outside.entry(c.text_of(toks[0]).to_string()).or_default().push(f);
        }
    }
    for (cls, cname) in &classes {
        if is_test(c, *cls, &by_name) {
            continue;
        }
        let mut routines: Vec<NodeId> =
            class_items(c, *cls).into_iter().filter(|&x| matches!(c.kind(x), FunctionDecl | TaskDecl)).collect();
        routines.extend(outside.get(cname).into_iter().flatten().copied());
        for f in routines {
            let Some(body) = c.tree.child(f, Body) else { continue };
            let m = c.tree.child(f, Name).map(|n| c.last_text(n).to_string()).unwrap_or_default();
            for call in subtree(c, body).into_iter().filter(|&x| c.kind(x) == CallExpr) {
                if !matches!(call_name(c, call), Some("srandom" | "set_randstate")) {
                    continue;
                }
                let Some(last) = c
                    .tree
                    .child(call, ArgList)
                    .and_then(|a| c.tree.children(a).filter(|&x| c.kind(x) == Arg).last())
                    .and_then(|a| c.tree.children(a).next())
                else {
                    continue;
                };
                if !is_constant(c, last, &params) || !ast_ok(c, call) {
                    continue;
                }
                let Some(at) = c.first_tok(call) else { continue };
                s.add(
                    "SV-TST-001",
                    "ConstantReseed",
                    Severity::Info,
                    format!(
                        "'{}' in {cname}::{m} reseeds with a constant, so this object produces the same random values \
                         for every simulation seed. Remove it, or derive the seed from the global seed (for example \
                         with get_full_name()).",
                        expr_text(c, call)
                    ),
                    c.loc(at),
                );
            }
        }
    }
}

// ---- SV-OOP-006 ----

/// The class name a data type names (`pkg::C #(T)` gives `C`).
pub(crate) fn type_name(c: &Cst, dt: NodeId) -> Option<String> {
    let first = c.first_tok(dt)?;
    if c.tree.tokens[first as usize].kind != sv_syntax::TokenKind::Identifier {
        return None;
    }
    let end = c
        .tree
        .children(dt)
        .find(|&k| matches!(c.kind(k), ParamList | Dim))
        .and_then(|p| c.first_tok(p))
        .map_or_else(|| c.tree.token_bounds(dt).map(|b| b.1), |p| Some(p - 1))?;
    Some(c.text_of(end).to_string())
}

/// The class around `n`, also for an out-of-block method body.
fn enclosing_class(c: &Cst, n: NodeId, classes: &HashMap<String, NodeId>) -> Option<NodeId> {
    let mut x = n;
    while let Some(p) = c.parent(x) {
        match c.kind(p) {
            ClassDecl => return Some(p),
            FunctionDecl | TaskDecl => {
                if let Some(nm) = c.tree.child(p, Name) {
                    let toks = c.tree.token_range(nm);
                    if toks.len() == 3 && c.text_of(toks[1]) == "::" {
                        return classes.get(c.text_of(toks[0])).copied();
                    }
                }
            }
            _ => {}
        }
        x = p;
    }
    None
}

/// A declarator without unpacked dimensions named `name` among `decl`s.
fn find_decl(c: &Cst, decls: impl Iterator<Item = NodeId>, name: &str) -> Option<NodeId> {
    for d in decls {
        if !matches!(c.kind(d), DataDecl | Port) || qualifiers(c, d).contains(&"typedef") {
            continue;
        }
        for (dcl, v, _) in declarators(c, d) {
            if v == name {
                if c.tree.children(dcl).any(|k| c.kind(k) == Dim) {
                    return None;
                }
                return c.tree.child(d, DataType);
            }
        }
    }
    None
}

/// Class properties of `cls` and its bases: their data types.
fn class_props(c: &Cst, cls: NodeId, classes: &HashMap<String, NodeId>) -> Vec<(String, Option<NodeId>)> {
    let mut out = Vec::new();
    let mut cur = Some(cls);
    let mut guard = 0;
    while let Some(k) = cur {
        for d in class_items(c, k).into_iter().filter(|&d| c.kind(d) == DataDecl) {
            if qualifiers(c, d).contains(&"typedef") {
                continue;
            }
            for (dcl, v, _) in declarators(c, d) {
                let dims = c.tree.children(dcl).any(|x| c.kind(x) == Dim);
                out.push((v, if dims { None } else { c.tree.child(d, DataType) }));
            }
        }
        guard += 1;
        cur = if guard < 64 { base_name(c, k).and_then(|b| classes.get(&b).copied()) } else { None };
    }
    out
}

pub(crate) fn is_class_type(c: &Cst, dt: Option<NodeId>, classes: &HashMap<String, NodeId>) -> bool {
    dt.and_then(|d| type_name(c, d))
        .is_some_and(|n| classes.contains_key(&n) || matches!(n.as_str(), "mailbox" | "semaphore" | "process"))
}

/// The class a copy's source names: `this`, or a variable of class type.
fn source_class(c: &Cst, src: NodeId, classes: &HashMap<String, NodeId>) -> Option<NodeId> {
    if c.kind(src) != NameExpr {
        return None;
    }
    let name = c.tree.bare_text(src);
    if name == "this" {
        return enclosing_class(c, src, classes);
    }
    let mut x = src;
    while let Some(p) = c.parent(x) {
        let found = match c.kind(p) {
            Body | SeqBlock | ModuleDecl | PackageDecl | ProgramDecl | InterfaceDecl => {
                find_decl(c, c.tree.children(p), &name)
            }
            FunctionDecl | TaskDecl => {
                find_decl(c, c.tree.child(p, PortList).into_iter().flat_map(|l| c.tree.children(l)), &name)
            }
            ClassDecl => class_props(c, p, classes).into_iter().find(|(v, _)| *v == name).and_then(|(_, d)| d),
            _ => None,
        };
        if let Some(dt) = found {
            return type_name(c, dt).and_then(|n| classes.get(&n).copied());
        }
        if matches!(c.kind(p), FunctionDecl | TaskDecl) {
            if let Some(cls) = enclosing_class(c, x, classes).filter(|&k| c.parent(p) != Some(k)) {
                let dt = class_props(c, cls, classes).into_iter().find(|(v, _)| *v == name).and_then(|(_, d)| d);
                return dt.and_then(|d| type_name(c, d)).and_then(|n| classes.get(&n).copied());
            }
        }
        x = p;
    }
    None
}

fn shallow_copy(c: &Cst, s: &mut Sink) {
    let classes: HashMap<String, NodeId> = class_decls(c)
        .into_iter()
        .filter(|&k| !c.tree.child(k, Header).is_some_and(|h| c.tok(h, "::").is_some()))
        .filter_map(|k| Some((decl_name(c, k)?.0, k)))
        .collect();
    for n in c.nodes(NewExpr) {
        let k = kids(c, n);
        let [src] = k.as_slice() else { continue };
        if c.kind(*src) == ArgList {
            continue;
        }
        let Some(cls) = source_class(c, *src, &classes) else { continue };
        if !class_props(c, cls, &classes).iter().any(|(_, dt)| is_class_type(c, *dt, &classes)) {
            continue;
        }
        if !ast_ok(c, n) {
            continue;
        }
        let Some(at) = c.first_tok(n) else { continue };
        let cname = decl_name(c, cls).map(|x| x.0).unwrap_or_default();
        s.add(
            "SV-OOP-006",
            "ClassShallowCopyAliasing",
            Severity::Error,
            format!(
                "Shallow copy 'new {}' on class '{cname}' duplicates primitive fields but copies nested class handles by \
                 reference. Both instances will share the same nested object handles in memory, creating data corruption \
                 and race conditions. Implement a deep copy method (e.g. clone() or copy()) instead.",
                c.tree.text(*src)
            ),
            c.loc(at),
        );
    }
}
