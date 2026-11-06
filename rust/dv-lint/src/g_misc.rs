//! Group G of builder slice N6.3: performance, error handling, dead code,
//! portability, security, DPI and file rules, ported from
//! `src/rules/{performance,errors,deadcode,portability,security,dpi}`.
//!
//! The C++ rules that walk slang's syntax are matched node for node; the
//! ones that walk the AST are matched on the syntax that AST comes from
//! (slang binds names, sv-syntax does not, so each says how it resolves
//! one). Messages are verbatim.

use std::collections::HashSet;

use sv_syntax::{NodeId, SyntaxKind};

use crate::syntax::Cst;
use crate::Check::Syntax;
use crate::{Rule, Severity, Sink};
use SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "DPI-TYP-001", name: "DpiTypeWidthAlignment", check: Syntax(dpi_type_width) },
        Rule { id: "SV-LOC-001", name: "ArrayLocatorEmptyQueueDeref", check: Syntax(locator_deref) },
        Rule { id: "SV-PRF-011", name: "DynamicArrayNewInLoop", check: Syntax(new_array_in_loop) },
        Rule { id: "SV-PRF-013", name: "ForkInClockedLoop", check: Syntax(fork_in_clocked_loop) },
        Rule { id: "SV-SYS-001", name: "UncheckedFopen", check: Syntax(unchecked_fopen) },
        Rule { id: "SV-DED-005", name: "UnreadClassProperty", check: Syntax(unread_property) },
        Rule { id: "SV-DED-006", name: "UnwrittenClassProperty", check: Syntax(unwritten_property) },
        Rule { id: "SV-DED-011", name: "EmptyTaskOrFunction", check: Syntax(empty_routine) },
        Rule { id: "SV-MOD-001", name: "GlobalUnitPollution", check: Syntax(unit_pollution) },
        Rule { id: "SV-MAC-001", name: "HeaderMacroLeak", check: Syntax(header_macro_leak) },
    ]
}

// ---- helpers ----

fn kids(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.tree.children(n).collect()
}

/// Every node under `n` (itself included), in source order.
fn subtree(c: &Cst, n: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![n];
    while let Some(x) = stack.pop() {
        out.push(x);
        let k = kids(c, x);
        stack.extend(k.into_iter().rev());
    }
    out
}

fn last_tok(c: &Cst, n: NodeId) -> Option<u32> {
    c.tree.token_bounds(n).map(|(_, b)| b)
}

fn is_ident(c: &Cst, tok: u32) -> bool {
    matches!(
        c.tree.tokens[tok as usize].kind,
        sv_lexer::Kind::Identifier | sv_lexer::Kind::EscapedIdentifier
    )
}

/// slang's `toString()` of the tokens `a..=b`: each with its leading trivia.
fn span_text(c: &Cst, a: u32, b: u32) -> String {
    let mut s = String::new();
    for t in a..=b {
        let tok = &c.tree.tokens[t as usize];
        if !tok.from_macro {
            s.push_str(c.tree.trivia(t));
        }
        s.push_str(&tok.text);
    }
    s
}

/// Whether slang parses `n` as a name (`a`, `a.b`, `p::c`, `a[i].b`)
/// rather than an expression on a value: a select on a name is part of the
/// name (`IdentifierSelectName`), not an `ElementSelectExpression`.
fn is_name(c: &Cst, n: NodeId) -> bool {
    match c.kind(n) {
        NameExpr => c.first_tok(n).is_some_and(|t| {
            is_ident(c, t) || matches!(c.text_of(t), "this" | "super" | "$unit" | "$root" | "local")
        }),
        ScopedExpr => true,
        MemberExpr | SelectExpr => kids(c, n).first().is_some_and(|&b| is_name(c, b)),
        _ => false,
    }
}

/// The loops whose body slang's loop visitors search: for, foreach,
/// while, repeat, forever (LoopStmt) and do-while; with the body.
fn loop_body(c: &Cst, n: NodeId) -> Option<NodeId> {
    match c.kind(n) {
        LoopStmt => kids(c, n).last().copied(),
        DoWhileStmt => kids(c, n).first().copied(),
        _ => None,
    }
}

// ---- DPI ----

fn dpi_type_width(c: &Cst, s: &mut Sink) {
    for n in c.nodes(DpiImport) {
        let Some(kw) = c.toks(n).find(|&t| matches!(c.text_of(t), "function" | "task")) else { continue };
        let Some(name) = c.tree.child(n, Name) else { continue };
        let Some(end) = last_tok(c, n) else { continue };
        let end = if c.text_of(end) == ";" { end - 1 } else { end };
        let proto = span_text(c, kw, end);
        if !(proto.contains("integer") || proto.contains(" reg ") || proto.contains("(reg ")) {
            continue;
        }
        let Some(at) = c.first_tok(name) else { continue };
        s.add(
            "DPI-TYP-001",
            "DpiTypeWidthAlignment",
            Severity::Warning,
            format!(
                "DPI-C function '{}' uses 4-state type ('integer'/'reg') across the C boundary. \
                 In IEEE 1800 § 35.5, 4-state types require complex 'svLogicVecVal' struct layouts in C/C++ FFI. \
                 Use portable 2-state types ('int' for int32_t, 'longint' for int64_t, 'byte' for int8_t) for direct C99 ABI compatibility.",
                c.text_of(at)
            ),
            c.loc(at),
        );
    }
}

// ---- array locators ----

const LOCATORS: &[&str] = &[
    "find",
    "find_index",
    "find_first",
    "find_first_index",
    "find_last",
    "find_last_index",
    "min",
    "max",
    "unique",
    "unique_index",
];

/// `getLocatorMethodName`: through parentheses and `with` clauses to the
/// method's name, else the expression's last token.
fn locator_name(c: &Cst, n: NodeId) -> Option<String> {
    match c.kind(n) {
        ParenExpr => {
            return kids(c, n).first().and_then(|&x| locator_name(c, x));
        }
        CallExpr => {
            let callee = kids(c, n).first().copied()?;
            if c.kind(callee) == CallExpr {
                // `x.find(..) with (..)`: slang's ArrayOrRandomizeMethod.
                return locator_name(c, callee);
            }
            let name = c.last_text(callee);
            if LOCATORS.contains(&name) {
                return Some(name.to_string());
            }
        }
        _ => {}
    }
    let name = c.last_text(n);
    LOCATORS.contains(&name).then(|| name.to_string())
}

fn locator_deref(c: &Cst, s: &mut Sink) {
    for n in c.nodes(SelectExpr) {
        let Some(&base) = kids(c, n).first() else { continue };
        if is_name(c, base) {
            continue;
        }
        let Some(name) = locator_name(c, base) else { continue };
        let Some(open) = c.tok(n, "[") else { continue };
        s.add(
            "SV-LOC-001",
            "ArrayLocatorEmptyQueueDeref",
            Severity::Error,
            format!(
                "Array locator method '{name}()' returns an unpacked queue which is immediately indexed with '[...]' without checking for emptiness. If no elements match, an empty queue is returned and accessing an index causes an out-of-bounds error per IEEE 1800 § 7.12.1. Store the locator result in a queue and check 'if (res.size() > 0)' before dereferencing."
            ),
            c.loc(open),
        );
    }
}

// ---- allocation and threads in loops ----

/// `new [n]` and `new [n](init)` (slang's NewArrayExpression).
fn is_new_array(c: &Cst, n: NodeId) -> bool {
    c.kind(n) == NewExpr && c.tok(n, "[").is_some()
}

fn new_array_in_loop(c: &Cst, s: &mut Sink) {
    // Each enclosing loop reports again, as each loop's finder does.
    for l in c.nodes(LoopStmt).into_iter().chain(c.nodes(DoWhileStmt)) {
        let Some(body) = loop_body(c, l) else { continue };
        for n in subtree(c, body) {
            if !is_new_array(c, n) {
                continue;
            }
            let Some(at) = c.first_tok(n) else { continue };
            s.add(
                "SV-PRF-011",
                "DynamicArrayNewInLoop",
                Severity::Warning,
                "Dynamic array allocation 'new[...]' is invoked inside a simulation loop. \
                 In SystemVerilog performance optimization, allocating dynamic \
                 array heap storage inside simulation loops causes continuous heap memory churn, heap lock contention, and fragmentation. \
                 Pre-allocate array capacity prior to loop entry, or reuse an existing pre-allocated buffer/queue."
                    .into(),
                c.loc(at),
            );
        }
    }
}

/// A clock edge that gates `n`: a posedge/negedge event outside any fork.
fn has_edge_outside_fork(c: &Cst, n: NodeId) -> bool {
    if c.kind(n) == ParBlock {
        return false;
    }
    if c.kind(n) == EventExpr && c.toks(n).any(|t| matches!(c.text_of(t), "posedge" | "negedge")) {
        return true;
    }
    c.tree.children(n).any(|k| has_edge_outside_fork(c, k))
}

fn in_clocked_loop(c: &Cst, fork: NodeId) -> bool {
    let mut cur = c.parent(fork);
    while let Some(p) = cur {
        let kw = c.keyword(p).map(|t| c.text_of(t)).unwrap_or("");
        match c.kind(p) {
            ProceduralBlock if kw == "always_ff" => return true,
            ProceduralBlock if kw == "always" => {
                if has_edge_outside_fork(c, p) {
                    return true;
                }
            }
            LoopStmt | DoWhileStmt => {
                if has_edge_outside_fork(c, p) {
                    return true;
                }
            }
            _ => {}
        }
        cur = c.parent(p);
    }
    false
}

fn fork_in_clocked_loop(c: &Cst, s: &mut Sink) {
    for n in c.nodes(ParBlock) {
        let Some(fork) = c.tok(n, "fork") else { continue };
        if !in_clocked_loop(c, n) {
            continue;
        }
        let join = c
            .toks(n)
            .find(|&t| matches!(c.text_of(t), "join" | "join_any" | "join_none"))
            .map(|t| c.text_of(t))
            .unwrap_or("");
        s.add(
            "SV-PRF-013",
            "ForkInClockedLoop",
            Severity::Warning,
            format!(
                "fork ... {join} sits inside a clocked loop, so every clock starts another thread. \
                 Do the work in the clocked process, or start the fork once outside the loop."
            ),
            c.loc(fork),
        );
    }
}

// ---- $fopen ----

fn unchecked_fopen(c: &Cst, s: &mut Sink) {
    for n in c.nodes(CallExpr) {
        let Some(&callee) = kids(c, n).first() else { continue };
        if c.trimmed(callee) != "$fopen" {
            continue;
        }
        let Some(at) = c.first_tok(n) else { continue };
        let Some(p) = c.parent(n) else { continue };
        if c.kind(p) == ExprStmt {
            s.add(
                "SV-SYS-001",
                "UncheckedFopen",
                Severity::Warning,
                "Return descriptor of '$fopen' is discarded. In SystemVerilog (IEEE 1800 § 21.3.1), '$fopen' returns a 32-bit descriptor; \
                 discarding the return value leaks file handles and prevents error validation."
                    .into(),
                c.loc(at),
            );
            continue;
        }
        // `fd = $fopen(...)`: slang's AssignmentExpression.
        if c.kind(p) != BinaryExpr || c.toks(p).next().map(|t| c.text_of(t)) != Some("=") {
            continue;
        }
        let Some(&lhs) = kids(c, p).first() else { continue };
        if lhs == n {
            continue;
        }
        let var = c.last_text(lhs).to_string();
        let mut enclosing = c.parent(p);
        while let Some(e) = enclosing {
            if matches!(c.kind(e), FunctionDecl | TaskDecl | SeqBlock | ModuleDecl) {
                break;
            }
            enclosing = c.parent(e);
        }
        let Some(e) = enclosing else { continue };
        let checked = subtree(c, e).into_iter().any(|x| {
            c.kind(x) == IfStmt && kids(c, x).first().is_some_and(|&cond| c.text(cond).contains(var.as_str()))
        });
        if checked {
            continue;
        }
        s.add(
            "SV-SYS-001",
            "UncheckedFopen",
            Severity::Warning,
            format!(
                "File descriptor '{var}' returned by '$fopen' is not verified against 0/NULL (e.g. 'if ({var} == 0)'). \
                 If the file cannot be opened, subsequent writes will crash the simulator or result in lost log data."
            ),
            c.loc(at),
        );
    }
}

// ---- class properties read or written ----

/// What the C++ property-usage visitors collect from a class's methods.
#[derive(Default)]
struct Usage {
    written: HashSet<String>,
    read: HashSet<String>,
}

/// slang's `IdentifierNameSyntax` reads, assignments and increments, as
/// `ClassPropertyUsageVisitor` (`unwritten` false) and
/// `ClassPropertyReadWriteVisitor` (`unwritten` true) record them.
fn usage(c: &Cst, n: NodeId, unwritten: bool, u: &mut Usage) {
    let k = kids(c, n);
    match c.kind(n) {
        BinaryExpr => {
            let op = c.toks(n).next().map(|t| c.text_of(t)).unwrap_or("");
            if matches!(op, "=" | "+=" | "-=" | "*=" | "/=") && k.len() == 2 {
                let lhs = c.last_text(k[0]).to_string();
                if op != "=" {
                    u.read.insert(lhs.clone());
                }
                u.written.insert(lhs);
                usage(c, k[1], unwritten, u);
                return;
            }
        }
        PrefixExpr | PostfixExpr => {
            if let Some(&operand) = k.first() {
                let name = c.last_text(operand).to_string();
                u.written.insert(name.clone());
                if !unwritten {
                    return;
                }
                u.read.insert(name);
            }
        }
        NameExpr => {
            if let Some(t) = c.first_tok(n) {
                let base_of_select = c
                    .parent(n)
                    .is_some_and(|p| c.kind(p) == SelectExpr && kids(c, p).first() == Some(&n));
                if is_ident(c, t) && !base_of_select {
                    u.read.insert(c.text_of(t).to_string());
                }
            }
        }
        MemberExpr | ScopedExpr => {
            // The name after `.` or `::` is an IdentifierName when the whole
            // is a name and no select or parameter list follows it.
            let base_of_select = c
                .parent(n)
                .is_some_and(|p| c.kind(p) == SelectExpr && kids(c, p).first() == Some(&n));
            let whole_name = c.kind(n) == ScopedExpr || k.first().is_some_and(|&b| is_name(c, b));
            if whole_name && !base_of_select && c.tree.child(n, ParamList).is_none() {
                if let Some(t) = last_tok(c, n).filter(|&t| is_ident(c, t)) {
                    u.read.insert(c.text_of(t).to_string());
                }
            }
        }
        LoopStmt if c.keyword(n).is_some_and(|t| c.text_of(t) == "foreach") => {
            // `foreach (a.b[i])`: the array is a name, its identifiers are read.
            let toks: Vec<u32> = c.toks(n).collect();
            for &t in toks.iter().skip(2) {
                if c.text_of(t) == "[" {
                    break;
                }
                if is_ident(c, t) {
                    u.read.insert(c.text_of(t).to_string());
                }
            }
        }
        _ => {}
    }
    for x in k {
        usage(c, x, unwritten, u);
    }
}

/// The class's non-rand properties (name, name token, has initializer) and
/// the usage in its methods.
fn class_properties(c: &Cst, cls: NodeId, unwritten: bool) -> (Vec<(String, u32, bool)>, Usage) {
    let mut props: Vec<(String, u32, bool)> = Vec::new();
    let mut u = Usage::default();
    for item in kids(c, cls) {
        match c.kind(item) {
            DataDecl => {
                if c.toks(item).any(|t| matches!(c.text_of(t), "rand" | "randc")) {
                    continue;
                }
                for d in kids(c, item) {
                    if c.kind(d) != Declarator {
                        continue;
                    }
                    let Some(name) = c.tree.child(d, Name) else { continue };
                    let Some(t) = c.first_tok(name) else { continue };
                    let init = c.tok(d, "=").is_some();
                    let n = c.text_of(t).to_string();
                    // One entry per name; a later declarator wins.
                    props.retain(|p| p.0 != n);
                    props.push((n, t, init));
                }
            }
            FunctionDecl | TaskDecl => usage(c, item, unwritten, &mut u),
            _ => {}
        }
    }
    (props, u)
}

fn unread_property(c: &Cst, s: &mut Sink) {
    for cls in c.nodes(ClassDecl) {
        let Some(name) = c.tree.child(cls, Name) else { continue };
        let class = c.tree.bare_text(name);
        let (props, u) = class_properties(c, cls, false);
        for (p, t, _) in props {
            if u.written.contains(&p) && !u.read.contains(&p) {
                s.add(
                    "SV-DED-005",
                    "UnreadClassProperty",
                    Severity::Info,
                    format!(
                        "Class property '{p}' is assigned or updated, but its value is never read in class '{class}'. Unread properties waste heap memory and indicate dead instrumentation or redundant state."
                    ),
                    c.loc(t),
                );
            }
        }
    }
}

fn unwritten_property(c: &Cst, s: &mut Sink) {
    for cls in c.nodes(ClassDecl) {
        let Some(name) = c.tree.child(cls, Name) else { continue };
        let class = c.tree.bare_text(name);
        let (props, u) = class_properties(c, cls, true);
        for (p, t, init) in props {
            if !init && u.read.contains(&p) && !u.written.contains(&p) {
                s.add(
                    "SV-DED-006",
                    "UnwrittenClassProperty",
                    Severity::Warning,
                    format!(
                        "Class property '{p}' is read in class '{class}' without an inline default initializer and is never assigned or initialized in any method. \
                         It will consistently evaluate to its uninitialized default value (0 or 'x), risking dead execution paths. \
                         Initialize the property at declaration or assign it inside the constructor 'new()'."
                    ),
                    c.loc(t),
                );
            }
        }
    }
}

// ---- empty routines ----

fn empty_routine(c: &Cst, s: &mut Sink) {
    for n in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)) {
        let Some(name) = c.tree.child(n, Name) else { continue };
        let task = c.kind(n) == TaskDecl;
        if c.tok(n, if task { "endtask" } else { "endfunction" }).is_none() {
            continue;
        }
        let in_class = c.parent(n).is_some_and(|p| c.kind(p) == ClassDecl);
        if in_class && c.toks(n).any(|t| matches!(c.text_of(t), "pure" | "virtual" | "extern")) {
            continue;
        }
        let routine = c.trimmed(name);
        if routine == "body" || routine == "new" {
            continue;
        }
        let has_stmt = c
            .tree
            .child(n, Body)
            .is_some_and(|b| c.tree.children(b).any(|x| c.kind(x) != EmptyStmt));
        if has_stmt {
            continue;
        }
        let Some(at) = c.first_tok(name) else { continue };
        s.add(
            "SV-DED-011",
            "EmptyTaskOrFunction",
            Severity::Warning,
            format!(
                "{} '{routine}' has an empty body with zero executable statements. \
                 Concrete routines with empty bodies execute as no-ops, consume simulation call-stack overhead, \
                 and typically represent unfinished testbench scaffolding or forgotten implementation stubs.",
                if task { "Task" } else { "Function" }
            ),
            c.loc(at),
        );
    }
}

// ---- $unit and header hygiene ----

const NET_TYPES: &[&str] = &[
    "wire", "tri", "wand", "wor", "triand", "trior", "tri0", "tri1", "trireg", "uwire", "supply0", "supply1",
    "interconnect",
];

fn unit_pollution(c: &Cst, s: &mut Sink) {
    let root = c.tree.root;
    for item in kids(c, root) {
        let (kind, names): (&str, Vec<NodeId>) = match c.kind(item) {
            DataDecl => {
                let net = c.toks(item).any(|t| NET_TYPES.contains(&c.text_of(t)));
                let decls = kids(c, item)
                    .into_iter()
                    .filter(|&d| c.kind(d) == Declarator)
                    .filter_map(|d| c.tree.child(d, Name))
                    .collect();
                (if net { "net" } else { "variable" }, decls)
            }
            TypedefDecl => ("typedef", c.tree.child(item, Name).into_iter().collect()),
            // An out-of-block body (`function C::f`) belongs to its class.
            FunctionDecl | TaskDecl | DpiImport => (
                "subroutine",
                c.tree.child(item, Name).filter(|&n| c.tok(n, "::").is_none()).into_iter().collect(),
            ),
            _ => continue,
        };
        for name in names {
            let Some(t) = last_tok(c, name) else { continue };
            s.add(
                "SV-MOD-001",
                "GlobalUnitPollution",
                Severity::Error,
                format!(
                    "Illegal declaration of {kind} '{}' in $unit compilation unit scope. All shared data types, variables, and routines \
                     must be encapsulated within a package (e.g. 'package my_pkg; ... endpackage') or module.",
                    c.text_of(t)
                ),
                c.loc(t),
            );
        }
    }
}

fn header_macro_leak(c: &Cst, s: &mut Sink) {
    let Some(first) = c.first_tok(c.tree.root) else { return };
    let file = c.tree.tokens[first as usize].origin.file;
    let Some(path) = c.tree.files.get(file as usize) else { return };
    let full = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
    let full = full.to_string_lossy().to_string();
    if !full.ends_with(".svh") {
        return;
    }
    let src: &str = &c.tree.sources[file as usize];
    let bytes = src.as_bytes();
    let at_start = |pos: usize| pos == 0 || matches!(bytes[pos - 1], b'\n' | b' ' | b'\t');
    let name_at = |from: usize, stops: &[u8]| -> Option<(usize, String)> {
        let start = from + bytes[from..].iter().position(|b| !matches!(b, b' ' | b'\t'))?;
        let end = bytes[start..].iter().position(|b| stops.contains(b)).map_or(bytes.len(), |e| start + e);
        Some((start, src[start..end].to_string()))
    };
    let mut defined: Vec<(String, usize)> = Vec::new();
    let mut undefined: HashSet<String> = HashSet::new();
    for (pos, _) in src.match_indices("`define") {
        if !at_start(pos) {
            continue;
        }
        let Some((start, name)) = name_at(pos + 7, b" \t\r\n(") else { continue };
        let guard = name.ends_with("_SVH") || name.ends_with("_H") || name.ends_with("_INC") || name.ends_with("_GUARD");
        let public = name.starts_with("uvm_") || name.starts_with("UVM_");
        if !guard && !public && !name.is_empty() {
            defined.push((name, start));
        }
    }
    for (pos, _) in src.match_indices("`undef") {
        if !at_start(pos) {
            continue;
        }
        if let Some((_, name)) = name_at(pos + 6, b" \t\r\n") {
            undefined.insert(name);
        }
    }
    for (name, off) in defined {
        if undefined.contains(&name) {
            continue;
        }
        s.add(
            "SV-MAC-001",
            "HeaderMacroLeak",
            Severity::Warning,
            format!(
                "Macro `{name} is defined in header file '{full}' without a corresponding `undef before file end. \
                 Leaking preprocessor definitions into global compilation scope causes accidental macro collisions across packages."
            ),
            c.loc_at(sv_syntax::Origin { file, offset: off as u32 }),
        );
    }
}
