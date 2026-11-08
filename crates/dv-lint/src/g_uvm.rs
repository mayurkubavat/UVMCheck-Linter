//! UVM and RAL rules (builder slice N6.3, group B), ported from
//! `cpp/src/rules/uvm/` and `cpp/src/rules/ral/`. Syntax rules here walk the
//! sv-syntax tree the way the C++ walks slang's syntax; see
//! [`crate::syntax`] for the shape differences. Messages are verbatim.

use sv_syntax::{Child, NodeId};

use crate::syntax::Cst;
use crate::Check::Syntax;
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "RAL-ACC-002", name: "MissingBackdoorAccessPathDefinition", check: Syntax(backdoor_path) },
        Rule { id: "RAL-COL-001", name: "RalAddressCollision", check: Syntax(ral_address_collision) },
        Rule { id: "RAL-MAP-001", name: "RalOrphanRegister", check: Syntax(ral_orphan_register) },
        Rule { id: "UVM-ALS-001", name: "UvmTransactionAliasing", check: Syntax(transaction_aliasing) },
        Rule { id: "UVM-PHS-006", name: "PhaseObjectionMismatch", check: Syntax(phase_objection_mismatch) },
        Rule { id: "UVM-PHS-008", name: "TimeInNonTimePhase", check: Syntax(time_in_non_time_phase) },
        Rule { id: "UVM-SEQ-004", name: "SequenceAbortSafety", check: Syntax(sequence_abort_safety) },
        Rule { id: "UVM-SEQ-006", name: "UncheckedGetResponse", check: Syntax(unchecked_get_response) },
        Rule { id: "UVM-SEQ-008", name: "SequenceItemDefaultNew", check: Syntax(sequence_item_default_new) },
    ]
}

// ---- helpers: slang's syntax shapes on sv-syntax ----

/// `n` and everything under it, in source order.
pub(crate) fn case_keyword(c: &Cst, n: NodeId) -> Option<u32> {
    c.toks(n).find(|&t| matches!(c.text_of(t), "case" | "casex" | "casez"))
}

pub(crate) fn subtree(c: &Cst, n: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![n];
    while let Some(x) = stack.pop() {
        out.push(x);
        let mut k: Vec<NodeId> = c.tree.children(x).collect();
        k.reverse();
        stack.extend(k);
    }
    out
}

/// A declaration's name (`cls.name`, a routine's last name token).
pub(crate) fn decl_name(c: &Cst, n: NodeId) -> String {
    c.tree.child(n, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default()
}

/// The Name node of a declaration.
pub(crate) fn name_node(c: &Cst, n: NodeId) -> Option<NodeId> {
    c.tree.child(n, Name)
}

/// Text of a routine's name token: `prototype->name->getLastToken()`.
pub(crate) fn routine_last_name(c: &Cst, f: NodeId) -> String {
    name_node(c, f).map(|x| c.last_text(x).to_string()).unwrap_or_default()
}

/// `prototype->name->getFirstToken()`.
pub(crate) fn routine_first_name(c: &Cst, f: NodeId) -> String {
    name_node(c, f).and_then(|x| c.first_tok(x)).map(|t| c.text_of(t).to_string()).unwrap_or_default()
}

/// Where `prototype->name` starts.
pub(crate) fn name_loc(c: &Cst, f: NodeId) -> Option<dv_model::Loc> {
    name_node(c, f).and_then(|x| c.first_tok(x)).map(|t| c.loc(t))
}

/// A class's `extends ...` clause (slang's `extendsClause`).
pub(crate) fn extends_clause(c: &Cst, cls: NodeId) -> Option<NodeId> {
    let h = c.tree.child(cls, Header)?;
    c.tree.child(h, Extends)
}

/// `extendsClause->baseName`: the base type as written.
pub(crate) fn base_name_node(c: &Cst, cls: NodeId) -> Option<NodeId> {
    c.tree.child(extends_clause(c, cls)?, DataType)
}

/// Functions and tasks declared directly in a class (`cls.items` that are
/// method declarations with a body).
pub(crate) fn class_routines(c: &Cst, cls: NodeId) -> Vec<NodeId> {
    c.tree.children(cls).filter(|&n| matches!(c.kind(n), FunctionDecl | TaskDecl)).collect()
}

/// A routine's body items (slang's `func.items`).
pub(crate) fn body_items(c: &Cst, f: NodeId) -> Vec<NodeId> {
    c.tree.child(f, Body).map(|b| c.items(b)).unwrap_or_default()
}

/// slang's `FunctionDeclarationSyntax::toString()`: from the `function` or
/// `task` keyword (qualifiers belong to the enclosing method declaration).
pub(crate) fn routine_text(c: &Cst, f: NodeId) -> String {
    let mut s = String::new();
    let mut on = false;
    for t in c.tree.token_range(f) {
        let tok = &c.tree.tokens[t as usize];
        if !on && matches!(&*tok.text, "function" | "task") {
            on = true;
        }
        if !on {
            continue;
        }
        if !tok.from_macro {
            s.push_str(c.tree.trivia(t));
        }
        s.push_str(&tok.text);
    }
    s
}

/// Drops `//` comments up to their line break (the C++ `stripComments`).
pub(crate) fn strip_line_comments(input: &str) -> String {
    let b = input.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut in_line = false;
    let mut i = 0;
    while i < b.len() {
        if !in_line && i + 1 < b.len() && b[i] == b'/' && b[i + 1] == b'/' {
            in_line = true;
            i += 1;
        } else if in_line && b[i] == b'\n' {
            in_line = false;
            out.push(b'\n');
        } else if !in_line {
            out.push(b[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub(crate) fn trim_ws(s: &str) -> &str {
    s.trim_matches([' ', '\t', '\r', '\n'])
}

/// `inv.left->getLastToken().valueText()`: the called name.
pub(crate) fn callee<'t>(c: &Cst<'t>, call: NodeId) -> &'t str {
    c.tree.children(call).next().map(|l| c.last_text(l)).unwrap_or("")
}

/// The call's argument list.
pub(crate) fn arg_list(c: &Cst, call: NodeId) -> Option<NodeId> {
    c.tree.child(call, ArgList)
}

/// slang's argument list parameters: each `Arg` (or an empty slot) in order.
pub(crate) fn params(c: &Cst, list: NodeId) -> Vec<Option<NodeId>> {
    let mut out = Vec::new();
    let mut cur: Option<Option<NodeId>> = None;
    for k in c.kids(list) {
        match *k {
            Child::Token(t) => match c.text_of(t) {
                "(" => cur = Some(None),
                "," | ")" => {
                    if let Some(x) = cur.take() {
                        out.push(x);
                    }
                    if c.text_of(t) == "," {
                        cur = Some(None);
                    }
                }
                _ => {}
            },
            Child::Node(n) => {
                if let Some(slot) = cur.as_mut() {
                    if c.kind(n) == Arg {
                        *slot = Some(n);
                    }
                }
            }
        }
    }
    // `f()` has no parameters, not one empty one.
    if out.len() == 1 && out[0].is_none() {
        out.clear();
    }
    out
}

/// An ordered argument's expression (not a named `.x(...)` one).
pub(crate) fn ordered_expr(c: &Cst, arg: NodeId) -> Option<NodeId> {
    if c.keyword(arg).is_some_and(|t| c.text_of(t) == ".") {
        return None;
    }
    c.items(arg).into_iter().next()
}

/// The first parameter when it is an ordered argument: its expression.
pub(crate) fn first_ordered(c: &Cst, call: NodeId) -> Option<NodeId> {
    let list = arg_list(c, call)?;
    ordered_expr(c, params(c, list).first().copied()??)
}

/// slang's `AssignmentExpression` (plain `=`): left and right.
pub(crate) fn assignment(c: &Cst, n: NodeId) -> Option<(u32, NodeId, NodeId)> {
    if c.kind(n) != BinaryExpr {
        return None;
    }
    let op = c.toks(n).next()?;
    if c.text_of(op) != "=" {
        return None;
    }
    let mut sides = c.tree.children(n);
    Some((op, sides.next()?, sides.next()?))
}

/// The expression of an expression statement.
pub(crate) fn stmt_expr(c: &Cst, s: NodeId) -> Option<NodeId> {
    (c.kind(s) == ExprStmt).then(|| c.items(s).into_iter().next()).flatten()
}

/// Innermost class declaration around `n`.
pub(crate) fn enclosing_class(c: &Cst, n: NodeId) -> Option<NodeId> {
    let mut cur = c.parent(n);
    while let Some(p) = cur {
        if c.kind(p) == ClassDecl {
            return Some(p);
        }
        cur = c.parent(p);
    }
    None
}

fn lower(s: &str) -> String {
    s.to_ascii_lowercase()
}

/// RAL rules' register block test on a class declaration.
fn is_reg_block_decl(c: &Cst, cls: NodeId) -> bool {
    let l = lower(&decl_name(c, cls));
    l.contains("reg_block")
        || l.contains("reg_model")
        || extends_clause(c, cls).is_some_and(|e| c.text(e).contains("uvm_reg_block"))
}

// ---- RAL ----

fn backdoor_path(c: &Cst, s: &mut Sink) {
    for n in c.nodes(CallExpr) {
        let method = callee(c, n);
        if !matches!(method, "add_hdl_path" | "set_hdl_path_root" | "add_hdl_path_slice" | "set_default_hdl_path") {
            continue;
        }
        let Some(expr) = first_ordered(c, n) else { continue };
        let path = c.trimmed(expr);
        if path == "\"\"" || path == "null" {
            let Some(at) = c.first_tok(expr) else { continue };
            s.add(
                "RAL-ACC-002",
                "MissingBackdoorAccessPathDefinition",
                Severity::Warning,
                format!(
                    "RAL backdoor path configuration '{method}(\"\")' specifies an empty or null HDL path string. \
                     In UVM RAL architecture (IEEE 1800.2 § 18.8), register backdoor read/write operations \
                     require a non-empty HDL root path to resolve target register signals within the simulation hierarchy."
                ),
                c.loc(at),
            );
        }
    }
}

/// `build()` routines declared directly in register block classes.
fn reg_block_builds(c: &Cst) -> Vec<(String, NodeId)> {
    let mut out = Vec::new();
    for cls in c.nodes(ClassDecl) {
        if !is_reg_block_decl(c, cls) {
            continue;
        }
        let name = decl_name(c, cls);
        for f in class_routines(c, cls) {
            if name_node(c, f).is_some() && routine_first_name(c, f) == "build" {
                out.push((name.clone(), f));
            }
        }
    }
    out
}

fn ral_address_collision(c: &Cst, s: &mut Sink) {
    for (cls, f) in reg_block_builds(c) {
        let body = strip_line_comments(&routine_text(c, f));
        let mut by_offset: std::collections::HashMap<String, String> = Default::default();
        let mut pos = 0;
        while let Some(at) = body[pos..].find(".add_reg").map(|i| i + pos) {
            pos = at;
            if let Some(open) = body[at..].find('(').map(|i| i + at) {
                if let Some(c1) = body[open..].find(',').map(|i| i + open) {
                    let c2 = body[c1 + 1..].find(',').map(|i| i + c1 + 1);
                    let close = body[c1 + 1..].find(')').map(|i| i + c1 + 1);
                    let end = match (c2, close) {
                        (Some(a), Some(b)) if a < b => Some(a),
                        (Some(a), None) => Some(a),
                        (_, b) => b,
                    };
                    if let Some(end) = end {
                        let reg = trim_ws(&body[open + 1..c1]);
                        let off = trim_ws(&body[c1 + 1..end]);
                        if !reg.is_empty() && !off.is_empty() {
                            if let Some(prev) = by_offset.get(off) {
                                let Some(loc) = name_loc(c, f) else { break };
                                s.add(
                                    "RAL-COL-001",
                                    "RalAddressCollision",
                                    Severity::Error,
                                    format!(
                                        "Register address collision in register block '{cls}': register '{reg}' is mapped to offset {off}, which is already occupied by register '{prev}'. Address collisions cause silent register overwrites and aliasing bugs."
                                    ),
                                    loc,
                                );
                            } else {
                                by_offset.insert(off.to_string(), reg.to_string());
                            }
                        }
                    }
                }
            }
            pos += 8;
        }
    }
}

fn ral_orphan_register(c: &Cst, s: &mut Sink) {
    for (cls, f) in reg_block_builds(c) {
        let body = strip_line_comments(&routine_text(c, f));
        let mut mapped = std::collections::HashSet::new();
        let mut pos = 0;
        while let Some(at) = body[pos..].find(".add_reg").map(|i| i + pos) {
            if let Some(open) = body[at..].find('(').map(|i| i + at) {
                if let Some(comma) = body[open..].find(',').map(|i| i + open) {
                    let reg = trim_ws(&body[open + 1..comma]);
                    if !reg.is_empty() {
                        mapped.insert(reg.to_string());
                    }
                }
            }
            pos = at + 8;
        }
        let mut made = std::collections::BTreeSet::new();
        let mut pos = 0;
        while let Some(at) = body[pos..].find("::type_id::create").map(|i| i + pos) {
            if let Some(eq) = body[..=at].rfind('=') {
                let start = body[..=eq].rfind('\n').map_or(0, |i| i + 1);
                let lhs = trim_ws(&body[start..eq]);
                if !lhs.is_empty() && !lhs.contains(' ') {
                    made.insert(lhs.to_string());
                }
            }
            pos = at + 18;
            if pos > body.len() {
                break;
            }
        }
        for reg in made {
            if mapped.contains(&reg) {
                continue;
            }
            let Some(loc) = name_loc(c, f) else { continue };
            s.add(
                "RAL-MAP-001",
                "RalOrphanRegister",
                Severity::Error,
                format!(
                    "Orphan register '{reg}' in register block '{cls}': instantiated in build() but never mapped via 'default_map.add_reg({reg}, ...)'. Orphan registers lack an address mapping, causing frontdoor write/read operations to fail."
                ),
                loc,
            );
        }
    }
}

// ---- UVM ----

fn transaction_aliasing(c: &Cst, s: &mut Sink) {
    let mut lists: Vec<Vec<NodeId>> = Vec::new();
    for f in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)) {
        lists.push(body_items(c, f));
    }
    for b in c.nodes(SeqBlock).into_iter().chain(c.nodes(ParBlock)) {
        lists.push(c.items(b));
    }
    for items in lists {
        let mut written: std::collections::HashSet<String> = Default::default();
        for item in items {
            let Some(e) = stmt_expr(c, item) else { continue };
            if c.kind(e) == CallExpr && callee(c, e) == "write" {
                if let Some(arg) = first_ordered(c, e) {
                    let tr = c.last_text(arg);
                    if !tr.is_empty() && tr != "this" {
                        written.insert(tr.to_string());
                    }
                }
            }
            let Some((_, lhs, rhs)) = assignment(c, e) else { continue };
            let l = c.last_text(lhs).to_string();
            let r = c.text(rhs);
            if r.contains("new") || r.contains("clone") {
                written.remove(&l);
            }
            let base = match c.kind(lhs) {
                MemberExpr | ScopedExpr => c.tree.children(lhs).next().map(|x| c.last_text(x).to_string()),
                _ => None,
            };
            let Some(base) = base.filter(|b| !b.is_empty() && written.contains(b)) else { continue };
            let Some(at) = c.first_tok(lhs) else { continue };
            s.add(
                "UVM-ALS-001",
                "UvmTransactionAliasing",
                Severity::Warning,
                format!(
                    "Transaction handle '{base}' was broadcast to an analysis port ('write({base})') and subsequently mutated in-place without cloning. In UVM, analysis ports pass handles by reference; \
                     in-place mutations corrupt transactions already stored by downstream scoreboards and coverage collectors. \
                     Clone the transaction before writing or allocate a new instance with 'new()'."
                ),
                c.loc(at),
            );
            written.remove(&base);
        }
    }
}

fn routines(c: &Cst) -> Vec<NodeId> {
    let mut v: Vec<NodeId> = c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)).collect();
    v.sort_by_key(|n| n.0);
    v
}

fn phase_objection_mismatch(c: &Cst, s: &mut Sink) {
    for f in routines(c) {
        let (mut raise, mut drop, mut first) = (0usize, 0usize, None);
        for item in body_items(c, f) {
            for n in subtree(c, item) {
                if c.kind(n) != CallExpr {
                    continue;
                }
                match callee(c, n) {
                    "raise_objection" => {
                        raise += 1;
                        if first.is_none() {
                            first = c.first_tok(n);
                        }
                    }
                    "drop_objection" => drop += 1,
                    _ => {}
                }
            }
        }
        if raise > drop {
            let Some(at) = first else { continue };
            s.add(
                "UVM-PHS-006",
                "PhaseObjectionMismatch",
                Severity::Error,
                format!(
                    "UVM phase objection raised via 'raise_objection()' in '{}()' without matching 'drop_objection()'. Unpaired phase objections prevent phase completion and cause simulation to hang indefinitely.",
                    routine_last_name(c, f)
                ),
                c.loc(at),
            );
        }
    }
}

/// slang's TimingControlStatement, WaitStatement or WaitForkStatement.
fn timing_kind(c: &Cst, n: NodeId) -> Option<&'static str> {
    match c.kind(n) {
        TimedStmt => Some("timing control (#, @, or ##)"),
        WaitStmt => {
            let mut t = c.toks(n);
            match (t.next().map(|x| c.text_of(x)), t.next().map(|x| c.text_of(x))) {
                (Some("wait"), Some("fork")) => Some("wait fork"),
                (Some("wait"), _) => Some("wait (wait condition)"),
                _ => None,
            }
        }
        _ => None,
    }
}

fn time_in_non_time_phase(c: &Cst, s: &mut Sink) {
    const NON_TIME: [&str; 8] = [
        "build_phase",
        "connect_phase",
        "end_of_elaboration_phase",
        "start_of_simulation_phase",
        "extract_phase",
        "check_phase",
        "report_phase",
        "final_phase",
    ];
    for f in routines(c) {
        if name_node(c, f).is_none() {
            continue;
        }
        let name = routine_last_name(c, f);
        if !NON_TIME.contains(&name.as_str()) {
            continue;
        }
        let found = body_items(c, f)
            .into_iter()
            .find_map(|i| subtree(c, i).into_iter().find_map(|n| timing_kind(c, n).map(|k| (n, k))));
        let Some((n, what)) = found else { continue };
        let Some(at) = c.first_tok(n) else { continue };
        s.add(
            "UVM-PHS-008",
            "TimeInNonTimePhase",
            Severity::Error,
            format!(
                "Non-time-consuming UVM phase '{name}' contains time-consuming construct '{what}'. Non-run UVM phases execute as instantaneous functions in 0 simulation time. Timing controls inside these phases corrupt the UVM execution schedule."
            ),
            c.loc(at),
        );
    }
}

fn sequence_abort_safety(c: &Cst, s: &mut Sink) {
    for cls in c.nodes(ClassDecl) {
        let name = decl_name(c, cls);
        let ext = extends_clause(c, cls).map(|e| c.text(e)).unwrap_or_default();
        if !(ext.contains("uvm_sequence") || name.contains("sequence") || name.contains("seq")) {
            continue;
        }
        for f in class_routines(c, cls) {
            if name_node(c, f).is_none() {
                continue;
            }
            let text = strip_line_comments(&routine_text(c, f));
            let mut start = 0;
            while let Some(at) = text[start..].find("start_item").map(|i| i + start) {
                let finish = text[at..].find("finish_item").map(|i| i + at);
                if let Some(ret) = text[at..].find("return").map(|i| i + at) {
                    let after = text[ret + 6..].find(|ch| ch != ' ' && ch != '\t').map(|i| i + ret + 6);
                    if after.is_some_and(|a| text.as_bytes()[a] == b';') && finish.map_or(true, |f| ret < f) {
                        if let Some(loc) = name_loc(c, f) {
                            s.add(
                                "UVM-SEQ-004",
                                "SequenceAbortSafety",
                                Severity::Error,
                                format!(
                                    "Task '{}' in sequence class '{name}' calls start_item() but contains an early 'return;' before finish_item(). \
                                     Aborting a sequence handshake without calling finish_item() permanently leaves the driver-sequencer pipeline in an un-granted deadlock.",
                                    routine_first_name(c, f)
                                ),
                                loc,
                            );
                        }
                        break;
                    }
                }
                start = at + 10;
            }
        }
    }
}

fn unchecked_get_response(c: &Cst, s: &mut Sink) {
    for n in c.nodes(CallExpr) {
        if callee(c, n) != "get_response" {
            continue;
        }
        let target = enclosing_class(c, n).map(|k| decl_name(c, k)).filter(|x| !x.is_empty());
        let target = target.unwrap_or_else(|| "sequence".into());
        let Some(at) = c.first_tok(n) else { continue };
        s.add(
            "UVM-SEQ-006",
            "UncheckedGetResponse",
            Severity::Warning,
            format!(
                "UVM sequence class '{target}' calls 'get_response()'. In UVM, calling get_response() blocks the sequence thread until the driver provides a response via 'seq_item_port.item_done(rsp)' or 'put_response(rsp)'. If the driver is not implemented to return response items, the sequence will hang indefinitely."
            ),
            c.loc(at),
        );
    }
}

fn sequence_item_default_new(c: &Cst, s: &mut Sink) {
    for cls in c.nodes(ClassDecl) {
        let Some(base) = base_name_node(c, cls) else { continue };
        let base = c.last_text(base);
        if base != "uvm_sequence_item" && !base.contains("sequence_item") {
            continue;
        }
        let class = decl_name(c, cls);
        for f in class_routines(c, cls) {
            if c.kind(f) != FunctionDecl || routine_last_name(c, f) != "new" {
                continue;
            }
            let Some(ports) = c.tree.child(f, PortList) else { continue };
            let Some(port) = c.tree.children(ports).find(|&p| c.kind(p) == Port) else { continue };
            let Some(d) = c.tree.child(port, Declarator) else { continue };
            if c.tok(d, "=").is_some() {
                continue;
            }
            let Some(nm) = name_node(c, d) else { continue };
            let Some(at) = c.first_tok(nm) else { continue };
            let port_name = c.tree.bare_text(nm);
            s.add(
                "UVM-SEQ-008",
                "SequenceItemDefaultNew",
                Severity::Warning,
                format!(
                    "Constructor 'new' of sequence item class '{class}' defines argument '{port_name}' without a default value. In UVM, sequence item constructors must provide a default string name (e.g. 'string name = \"{class}\"') to support factory creation (`uvm_object_utils) and .clone() / .copy() operations."
                ),
                c.loc(at),
            );
        }
    }
}
