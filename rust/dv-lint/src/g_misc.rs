//! Group G of builder slice N6.3: performance, error handling, dead code,
//! portability, security, DPI and file rules, ported from
//! `cpp/src/rules/{performance,errors,deadcode,portability,security,dpi}`.
//!
//! The C++ rules that walk slang's syntax are matched node for node; the
//! ones that walk the AST are matched on the syntax that AST comes from
//! (slang binds names, sv-syntax does not, so each says how it resolves
//! one). Messages are verbatim.

use std::collections::{HashMap, HashSet};

use sv_syntax::{NodeId, SyntaxKind};

use crate::syntax::Cst;
use crate::Check::{Mixed, Syntax};
use crate::{Rule, Severity, Sink};
use dv_model::Design;
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
        Rule { id: "PORT-PTH-001", name: "HardcodedHostPaths", check: Mixed(ast_hardcoded_paths) },
        Rule { id: "SEC-LOG-001", name: "PlaintextKeyInLogs", check: Mixed(ast_plaintext_key) },
        Rule { id: "SV-PRF-005", name: "UnshieldedVerbosityFormatEval", check: Mixed(ast_unshielded_format) },
        Rule { id: "SV-PRF-004", name: "ProceduralZeroDelay", check: Mixed(ast_zero_delay) },
        Rule { id: "SV-PRF-003", name: "DynArrayIncrementalResizeInLoop", check: Mixed(ast_resize_in_loop) },
        Rule { id: "SV-ERR-003", name: "ProcessTerminationInClass", check: Syntax(termination_in_class) },
        Rule { id: "UVM-TST-002", name: "FixedDelayBeforeDrop", check: Syntax(fixed_delay_before_drop) },
        Rule { id: "SV-PRF-009", name: "NbaOnClassProperty", check: Mixed(ast_nba_on_property) },
        Rule { id: "SV-PRF-008", name: "QueueSlicePopFirst", check: Mixed(ast_queue_slice) },
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
            ProceduralBlock if kw == "always" && has_edge_outside_fork(c, p) => return true,
            LoopStmt | DoWhileStmt if has_edge_outside_fork(c, p) => return true,
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
    // A file of directives only has just the end-of-file token, which slang
    // places in the file itself.
    let file = c.first_tok(c.tree.root).map_or(0, |t| c.tree.tokens[t as usize].origin.file);
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

// ---- calls, as slang's AST sees them ----

/// A call: `f(...)`, `o.m(...)`, `p::f(...)`, or a bare `$name` (slang
/// calls system subroutines written without parentheses).
struct CallSite {
    node: NodeId,
    /// `getSubroutineName()`: the last name of the callee.
    name: String,
    /// Ordered arguments as written (slang's `arguments()` for system calls).
    args: Vec<Option<NodeId>>,
}

fn call_args(c: &Cst, call: NodeId) -> Vec<Option<NodeId>> {
    let Some(list) = c.tree.child(call, ArgList) else { return Vec::new() };
    let mut out = vec![None];
    for k in c.kids(list) {
        match *k {
            sv_syntax::Child::Token(t) if c.text_of(t) == "," => out.push(None),
            sv_syntax::Child::Node(a) => {
                *out.last_mut().unwrap() = if c.kind(a) == Arg { c.tree.children(a).next() } else { Some(a) }
            }
            _ => {}
        }
    }
    if out.len() == 1 && out[0].is_none() {
        out.clear();
    }
    out
}

fn call_site(c: &Cst, n: NodeId) -> Option<CallSite> {
    match c.kind(n) {
        CallExpr => {
            let callee = kids(c, n).first().copied()?;
            if c.kind(callee) == CallExpr {
                return None; // `x.m(..) with ..`: the inner call is the site
            }
            let name = c.last_text(callee).to_string();
            Some(CallSite { node: n, name, args: call_args(c, n) })
        }
        NameExpr => {
            let t = c.first_tok(n)?;
            if c.tree.tokens[t as usize].kind != sv_lexer::Kind::SystemIdentifier {
                return None;
            }
            if matches!(c.text_of(t), "$" | "$root" | "$unit") {
                return None;
            }
            let p = c.parent(n);
            if p.is_some_and(|p| matches!(c.kind(p), CallExpr | MemberExpr | ScopedExpr) && kids(c, p).first() == Some(&n)) {
                return None;
            }
            Some(CallSite { node: n, name: c.text_of(t).to_string(), args: Vec::new() })
        }
        _ => None,
    }
}

fn calls_in(c: &Cst, root: NodeId) -> Vec<CallSite> {
    subtree(c, root).into_iter().filter_map(|n| call_site(c, n)).collect()
}

/// slang's `toString()`: each token with its leading trivia (one space for
/// a macro-made token that had any).
fn source_text(c: &Cst, n: NodeId) -> String {
    let mut s = String::new();
    for t in c.tree.token_range(n) {
        let tok = &c.tree.tokens[t as usize];
        if !tok.from_macro {
            s.push_str(c.tree.trivia(t));
        } else if tok.space_before {
            s.push(' ');
        }
        s.push_str(&tok.text);
    }
    s
}

/// `swe::exprText`: tokens joined, any trivia between two of them one space.
fn norm_text(c: &Cst, n: NodeId) -> String {
    let mut s = String::new();
    for t in c.tree.token_range(n) {
        let tok = &c.tree.tokens[t as usize];
        let spaced = if tok.from_macro { tok.space_before } else { !c.tree.trivia(t).is_empty() };
        if !s.is_empty() && spaced {
            s.push(' ');
        }
        s.push_str(&tok.text);
    }
    s
}

/// A string literal's value (`"a\\tb"` -> `a<tab>b`).
fn string_value(c: &Cst, n: NodeId) -> Option<String> {
    if c.kind(n) != Literal {
        return None;
    }
    let (a, b) = c.tree.token_bounds(n)?;
    if a != b {
        return None;
    }
    let raw = c.text_of(a);
    let inner = raw.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::new();
    let mut it = inner.chars();
    while let Some(ch) = it.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('v') => out.push('\x0b'),
            Some('f') => out.push('\x0c'),
            Some('a') => out.push('\x07'),
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    Some(out)
}

/// The node through parentheses (slang binds no parenthesized expression).
fn unparen(c: &Cst, mut n: NodeId) -> NodeId {
    while c.kind(n) == ParenExpr {
        match kids(c, n).first() {
            Some(&x) => n = x,
            None => break,
        }
    }
    n
}

fn hardcoded_paths(c: &Cst, a: &Ast, s: &mut Sink) {
    for n in subtree(c, c.tree.root) {
        let Some(call) = call_site(c, n) else { continue };
        let name = call.name.strip_prefix('$').unwrap_or(&call.name);
        if !matches!(name, "readmemh" | "readmemb" | "writememh" | "writememb" | "fopen") {
            continue;
        }
        let Some(Some(arg)) = call.args.first().copied() else { continue };
        let arg = unparen(c, arg);
        let Some(path) = string_value(c, arg) else { continue };
        let b = path.as_bytes();
        let unix = path.starts_with('/');
        let win = b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/');
        if !(unix || win) {
            continue;
        }
        let Some(at) = c.first_tok(arg) else { continue };
        s.add(
            "PORT-PTH-001",
            "HardcodedHostPaths",
            Severity::Error,
            format!(
                "Hardcoded absolute host filesystem path '{path}' in system task '${name}'. \
                 Absolute paths break regression portability across Linux CI/CD containers and regression clouds. \
                 Use relative paths or parameterize the path via '$value$plusargs(\"path=%s\", ...)'."
            ),
            c.loc(at),
        );
        a.again(s, c, n);
    }
}

fn plaintext_key(c: &Cst, a: &Ast, s: &mut Sink) {
    for n in subtree(c, c.tree.root) {
        let Some(call) = call_site(c, n) else { continue };
        let name = call.name.strip_prefix('$').unwrap_or(&call.name);
        if !matches!(name, "sformatf" | "display" | "write" | "writeh") || call.args.len() < 2 {
            continue;
        }
        let Some(Some(fmt)) = call.args.first().copied() else { continue };
        let Some(fmt) = string_value(c, unparen(c, fmt)) else { continue };
        if !fmt.contains('%') {
            continue;
        }
        for arg in call.args.iter().skip(1).copied().flatten() {
            let text = source_text(c, arg);
            let low = text.to_lowercase();
            if !["key", "secret", "seed", "token", "priv_"].iter().any(|w| low.contains(w)) {
                continue;
            }
            // The text keeps the parentheses; the location is inside them.
            let Some(at) = c.first_tok(unparen(c, arg)) else { continue };
            s.add(
                "SEC-LOG-001",
                "PlaintextKeyInLogs",
                Severity::Warning,
                format!(
                    "Potential plaintext logging of sensitive cryptographic asset '{text}' via ${name}(). Printing keys, seeds, or root-of-trust secrets in simulation \
                     transcripts violates hardware security standards (ISO/SAE 21434 / Common Criteria). \
                     Mask or omit sensitive secrets from regression logs."
                ),
                c.loc(at),
            );
            a.again(s, c, n);
        }
    }
}

/// An integer literal's value, when it is one (`300`, `'d300`, `12'h12c`).
fn int_value(c: &Cst, n: NodeId) -> Option<u128> {
    if c.kind(n) != Literal {
        return None;
    }
    let toks = c.tree.token_range(n);
    let text: String = toks.iter().map(|&t| c.text_of(t)).collect::<String>().replace('_', "");
    let digits = match text.find('\'') {
        None => return text.parse().ok(),
        Some(i) => &text[i + 1..],
    };
    let digits = digits.trim_start_matches(['s', 'S']);
    let (radix, rest) = match digits.chars().next()? {
        'd' | 'D' => (10, &digits[1..]),
        'h' | 'H' => (16, &digits[1..]),
        'o' | 'O' => (8, &digits[1..]),
        'b' | 'B' => (2, &digits[1..]),
        _ => return None,
    };
    u128::from_str_radix(rest, radix).ok()
}

fn unshielded_format(c: &Cst, a: &Ast, s: &mut Sink) {
    fn walk(c: &Cst, a: &Ast, t: &Formals, n: NodeId, guard: usize, s: &mut Sink) {
        let mut guard = guard;
        if c.kind(n) == IfStmt
            && c.tree.children(n).next().is_some_and(|cond| source_text(c, cond).contains("uvm_report_enabled"))
        {
            guard += 1;
        }
        // Only a call named like uvm_info/uvm_warning can fire: test the
        // name before building the call site's arguments.
        let named = c.kind(n) == CallExpr
            && c.tree.children(n).next().is_some_and(|callee| {
                let t = c.last_text(callee);
                ["uvm_report_info", "uvm_info", "uvm_report_warning", "uvm_warning"].iter().any(|w| t.contains(w))
            });
        if guard == 0 && named {
            if let Some(call) = call_site(c, n) {
                if check(c, t, &call, s) {
                    a.again(s, c, n);
                }
            }
        }
        for k in c.tree.children(n) {
            walk(c, a, t, k, guard, s);
        }
    }
    fn check(c: &Cst, table: &Formals, call: &CallSite, s: &mut Sink) -> bool {
        let n = &call.name;
        let info_or_warning = ["uvm_report_info", "uvm_info", "uvm_report_warning", "uvm_warning"]
            .iter()
            .any(|w| n.contains(w));
        if !info_or_warning {
            return false;
        }
        // slang's arguments() has one per formal, defaults filled in.
        let ports = table.get(n.as_str()).cloned().unwrap_or_default();
        if ports.len().max(call.args.len()) < 2 {
            return false;
        }
        let Some(msg) = call.args.get(1).copied().flatten() else { return false };
        if !source_text(c, msg).contains("$sformatf") {
            return false;
        }
        // The verbosity: written, or the formal's default. An enum
        // constant bound to an int formal is a conversion with no syntax,
        // so in practice only a plain decimal literal counts.
        let verb = call.args.get(2).copied().flatten().or_else(|| ports.get(2).copied().flatten());
        let high = verb.is_some_and(|v| {
            let v = unparen(c, v);
            let toks = c.tree.token_range(v);
            c.kind(v) == Literal && toks.len() == 1 && c.text_of(toks[0]).bytes().all(|b| b.is_ascii_digit() || b == b'_')
                && int_value(c, v).is_some_and(|x| x > 200)
        });
        if !high {
            return false;
        }
        let Some(at) = c.first_tok(msg) else { return false };
        s.add(
            "SV-PRF-005",
            "UnshieldedVerbosityFormatEval",
            Severity::Warning,
            "Calling $sformatf inside uvm_info/warning with high verbosity without checking 'uvm_report_enabled'. \
             Because macro/subroutine arguments evaluate eagerly, expensive string formatting and heap allocations execute \
             on every transaction even when messages are suppressed by simulation verbosity. \
             Wrap call in 'if (uvm_report_enabled(...))' or use `uvm_info_begin/end."
                .into(),
            c.loc(at),
        );
        true
    }
    walk(c, a, &formals(c), c.tree.root, 0, s);
}

fn zero_delay(c: &Cst, a: &Ast, s: &mut Sink) {
    for n in c.nodes(TimedStmt) {
        let Some(tc) = c.tree.child(n, TimingControl) else { continue };
        let Some(hash) = c.tok(tc, "#") else { continue };
        let Some(expr) = kids(c, tc).first().copied() else { continue };
        let expr = unparen(c, expr);
        let zero = int_value(c, expr) == Some(0) || source_text(c, expr) == "0";
        if !zero {
            continue;
        }
        let at = c.first_tok(n).unwrap_or(hash);
        s.add(
            "SV-PRF-004",
            "ProceduralZeroDelay",
            Severity::Warning,
            "Procedural delay '#0' reschedules the process into the Inactive simulation region. \
             Cascades of #0 delays cause delta-cycle thrashing, inflate event engine queues, and mask underlying race conditions. \
             Use non-blocking assignments (<=), clocking block skews, or explicit event synchronization instead."
                .into(),
            c.loc(at),
        );
        a.again(s, c, n);
    }
}

fn resize_in_loop(c: &Cst, a: &Ast, s: &mut Sink) {
    for n in c.nodes(NewExpr) {
        // `new [n](init)`: the size, then the array to copy.
        if c.tok(n, "[").is_none() || c.tok(n, "(").is_none() {
            continue;
        }
        let mut cur = c.parent(n);
        let mut in_loop = false;
        while let Some(p) = cur {
            let kw = c.keyword(p).map(|t| c.text_of(t)).unwrap_or("");
            if (c.kind(p) == LoopStmt && matches!(kw, "for" | "foreach" | "while")) || c.kind(p) == DoWhileStmt {
                in_loop = true;
                break;
            }
            cur = c.parent(p);
        }
        if !in_loop {
            continue;
        }
        let Some(at) = c.first_tok(n) else { continue };
        s.add(
            "SV-PRF-003",
            "DynArrayIncrementalResizeInLoop",
            Severity::Warning,
            "Dynamic array resize 'new[...](existing_array)' invoked inside a loop. \
             Incrementally resizing dynamic arrays creates an O(N^2) heap reallocation and element-copying bottleneck. \
             Pre-allocate the full array with 'new[size]' before the loop, or use a SystemVerilog queue ([$])."
                .into(),
            c.loc(at),
        );
        a.again(s, c, n);
    }
}

// ---- class methods (swe::userClasses / swe::classMethods) ----

/// `swe::isLibrarySymbol` for a class: in package uvm_pkg, named uvm_*
/// (or inside such a class), or declared in uvm_pkg.sv or uvm_mock.svh.
fn is_library_class(c: &Cst, cls: NodeId) -> bool {
    let mut cur = Some(cls);
    while let Some(n) = cur {
        let name = c.tree.child(n, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default();
        if (c.kind(n) == PackageDecl && name == "uvm_pkg") || (c.kind(n) == ClassDecl && name.starts_with("uvm_")) {
            return true;
        }
        cur = c.parent(n);
    }
    let Some(t) = c.tree.child(cls, Name).and_then(|x| c.first_tok(x)) else { return false };
    let file = c.tree.tokens[t as usize].origin.file;
    let path = c.tree.files.get(file as usize).map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    path.contains("uvm_mock.svh") || path.contains("uvm_pkg.sv")
}

/// A class declaration, its name and its methods' (declaration, name).
type ClassMethods = (NodeId, String, Vec<(NodeId, String)>);

/// The user classes of the tree with their methods' declarations (in-class
/// bodies and out-of-block `C::m` bodies; no pure or DPI ones).
fn user_class_methods(c: &Cst) -> Vec<ClassMethods> {
    let mut out = Vec::new();
    let classes: Vec<NodeId> = c.nodes(ClassDecl).into_iter().filter(|&k| !is_library_class(c, k)).collect();
    let mut by_name: HashMap<String, usize> = HashMap::new();
    for &k in &classes {
        let name = c.tree.child(k, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default();
        let mut methods = Vec::new();
        for item in kids(c, k) {
            if matches!(c.kind(item), FunctionDecl | TaskDecl) {
                let m = c.tree.child(item, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default();
                methods.push((item, m));
            }
        }
        by_name.entry(name.clone()).or_insert(out.len());
        out.push((k, name, methods));
    }
    for n in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)) {
        let Some(name) = c.tree.child(n, Name) else { continue };
        let toks = c.tree.token_range(name);
        if toks.len() < 3 || c.text_of(toks[toks.len() - 2]) != "::" {
            continue;
        }
        let class = c.text_of(toks[toks.len() - 3]);
        if let Some(&i) = by_name.get(class) {
            out[i].2.push((n, c.text_of(toks[toks.len() - 1]).to_string()));
        }
    }
    out
}

fn termination_in_class(c: &Cst, s: &mut Sink) {
    for (_, class, methods) in user_class_methods(c) {
        for (m, name) in methods {
            let Some(body) = c.tree.child(m, Body) else { continue };
            for call in calls_in(c, body) {
                if !matches!(call.name.as_str(), "$finish" | "$stop" | "$exit") {
                    continue;
                }
                let Some(at) = c.first_tok(call.node) else { continue };
                s.add(
                    "SV-ERR-003",
                    "ProcessTerminationInClass",
                    Severity::Warning,
                    format!(
                        "{} in {class}::{name} ends the simulation without the check and report phases or the UVM \
                         summary, so regression scripts may read the log as a pass. Use `uvm_fatal \
                         to abort, or drop objections to end the test.",
                        call.name
                    ),
                    c.loc(at),
                );
            }
        }
    }
}

/// `swe::isConstantExpr` on syntax: literals, and operators on them (a
/// parameter name is constant too).
fn is_constant(c: &Cst, n: NodeId, params: &HashSet<String>) -> bool {
    let n = unparen(c, n);
    match c.kind(n) {
        Literal => true,
        NameExpr => c.first_tok(n).is_some_and(|t| params.contains(c.text_of(t))),
        BinaryExpr => kids(c, n).iter().all(|&k| is_constant(c, k, params)),
        PrefixExpr => kids(c, n).iter().all(|&k| is_constant(c, k, params)),
        _ => false,
    }
}

fn is_drop_stmt(c: &Cst, n: NodeId) -> bool {
    c.kind(n) == ExprStmt
        && kids(c, n).first().and_then(|&e| call_site(c, e)).is_some_and(|k| k.name == "drop_objection")
}

fn fixed_delay_before_drop(c: &Cst, s: &mut Sink) {
    let params: HashSet<String> = c
        .nodes(ParamDecl)
        .into_iter()
        .flat_map(|p| kids(c, p))
        .filter(|&d| c.kind(d) == Declarator)
        .filter_map(|d| c.tree.child(d, Name).map(|x| c.tree.bare_text(x)))
        .collect();
    let report = |t: NodeId, s: &mut Sink| {
        let Some(tc) = c.tree.child(t, TimingControl) else { return };
        let Some(e) = kids(c, tc).first().copied() else { return };
        let Some(at) = c.first_tok(t) else { return };
        s.add(
            "UVM-TST-002",
            "FixedDelayBeforeDrop",
            Severity::Warning,
            format!(
                "Fixed delay '{}' before drop_objection() guesses how long the testbench needs to drain. It is flaky \
                 when traffic grows and wastes time when it shrinks. Use the objection drain time \
                 (phase.phase_done.set_drain_time) or phase_ready_to_end() in the scoreboard.",
                norm_text(c, e)
            ),
            c.loc(at),
        );
    };
    let fixed = |t: NodeId| {
        c.tree.child(t, TimingControl).is_some_and(|tc| {
            c.tok(tc, "#").is_some() && kids(c, tc).first().is_some_and(|&e| is_constant(c, e, &params))
        })
    };
    for (_, _, methods) in user_class_methods(c) {
        for (m, _) in methods {
            let Some(body) = c.tree.child(m, Body) else { continue };
            for n in subtree(c, body) {
                if c.kind(n) == TimedStmt {
                    let inner = kids(c, n).get(1).copied();
                    if inner.is_some_and(|x| is_drop_stmt(c, x)) && fixed(n) {
                        report(n, s);
                    }
                }
                if matches!(c.kind(n), Body | SeqBlock | ParBlock) {
                    let items: Vec<NodeId> = c.items(n).into_iter().filter(|&x| c.kind(x) != DataDecl).collect();
                    for w in items.windows(2) {
                        let t = w[0];
                        if c.kind(t) != TimedStmt || !is_drop_stmt(c, w[1]) || !fixed(t) {
                            continue;
                        }
                        if kids(c, t).get(1).is_some_and(|&x| c.kind(x) == EmptyStmt) {
                            report(t, s);
                        }
                    }
                }
            }
        }
    }
}

// ---- how often slang's AST walk visits code ----

/// The engine walks the elaborated AST, so code in a parameterized class
/// is visited once per specialization. Maps each class declaration of a
/// tree to that count.
pub(crate) struct Ast {
    visits: HashMap<NodeId, u32>,
}

impl Ast {
    fn new(d: &Design, c: &Cst) -> Ast {
        let by_loc: HashMap<(u32, u32, u32), u32> = d
            .classes()
            .iter()
            .filter(|k| k.is_parameterized)
            .map(|k| ((k.loc.file, k.loc.line, k.loc.column), d.specializations(k.id)))
            .collect();
        let mut visits = HashMap::new();
        if !by_loc.is_empty() {
            for n in c.nodes(ClassDecl) {
                let Some(t) = c.tree.child(n, Name).and_then(|x| c.first_tok(x)) else { continue };
                let l = c.loc(t);
                if let Some(&k) = by_loc.get(&(l.file, l.line, l.column)) {
                    visits.insert(n, k);
                }
            }
        }
        Ast { visits }
    }

    /// Visits of node `n`: the product over the classes around it.
    fn times(&self, c: &Cst, n: NodeId) -> u32 {
        let mut k = 1;
        let mut cur = Some(n);
        while let Some(x) = cur {
            if let Some(&v) = self.visits.get(&x) {
                k *= v;
            }
            cur = c.parent(x);
        }
        k
    }

    /// Repeats the finding just added for each further visit of `n`.
    fn again(&self, s: &mut Sink, c: &Cst, n: NodeId) {
        let k = self.times(c, n);
        if k == 0 {
            s.findings.pop();
            return;
        }
        if let Some(last) = s.findings.last().cloned() {
            for _ in 1..k {
                s.findings.push(last.clone());
            }
        }
    }
}

macro_rules! ast_rules {
    ($($wrap:ident => $f:ident),* $(,)?) => {
        $(fn $wrap(d: &Design, csts: &[Cst], s: &mut Sink) {
            for c in csts {
                $f(c, &Ast::new(d, c), s);
            }
        })*
    };
}

ast_rules! {
    ast_hardcoded_paths => hardcoded_paths,
    ast_plaintext_key => plaintext_key,
    ast_unshielded_format => unshielded_format,
    ast_zero_delay => zero_delay,
    ast_resize_in_loop => resize_in_loop,
    ast_queue_slice => queue_slice,
}

/// The default value of each formal of the subroutines named `name` in the
/// tree (the first declaration found), `None` for a formal without one.
/// For each subroutine name, the default of each formal: the first
/// function, then task, then prototype of that name in the tree.
type Formals<'t> = HashMap<&'t str, Vec<Option<NodeId>>>;

fn formals<'t>(c: &Cst<'t>) -> Formals<'t> {
    let mut out = HashMap::new();
    for kind in [FunctionDecl, TaskDecl, MethodPrototype] {
        for n in c.nodes(kind) {
            let Some(nm) = c.tree.child(n, Name) else { continue };
            out.entry(c.last_text(nm)).or_insert_with(|| {
                let Some(list) = c.tree.child(n, PortList) else { return Vec::new() };
                c.tree
                    .children(list)
                    .filter(|&p| c.kind(p) == Port)
                    .map(|p| {
                        let d = c.tree.child(p, Declarator)?;
                        let eq = c.tok(d, "=")?;
                        c.node_after(d, eq)
                    })
                    .collect()
            });
        }
    }
    out
}

// ---- names, resolved as slang binds them (within one tree) ----

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DeclKind {
    /// A method or block local, a loop variable.
    Local,
    Arg,
    /// A class property.
    Prop,
    /// A module, interface, package or `$unit` variable or port.
    Other,
}

/// A declared value: what it is, its declarator (the identity slang's
/// symbol has) and its data type.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Decl {
    kind: DeclKind,
    item: NodeId,
    dtype: Option<NodeId>,
}

fn declarators_named(c: &Cst, decl: NodeId, name: &str) -> Option<NodeId> {
    kids(c, decl).into_iter().find(|&d| {
        c.kind(d) == Declarator && c.tree.child(d, Name).is_some_and(|x| c.tree.bare_text(x) == name)
    })
}

fn find_in_items(c: &Cst, scope: NodeId, name: &str, kind: DeclKind) -> Option<Decl> {
    for item in kids(c, scope) {
        if c.kind(item) != DataDecl {
            continue;
        }
        if let Some(d) = declarators_named(c, item, name) {
            return Some(Decl { kind, item: d, dtype: c.tree.child(item, DataType) });
        }
    }
    None
}

fn class_named(c: &Cst, name: &str) -> Option<NodeId> {
    c.first_named(ClassDecl, name)
}

/// The class `k` extends, by name, within the tree.
fn base_class(c: &Cst, k: NodeId) -> Option<NodeId> {
    let e = c.tree.child(k, Extends).or_else(|| c.tree.child(c.tree.child(k, Header)?, Extends))?;
    let dt = kids(c, e).into_iter().next()?;
    let toks = c.tree.token_range(dt);
    let end = toks.iter().position(|&t| c.text_of(t) == "#").unwrap_or(toks.len());
    let name = c.text_of(*toks[..end].last()?);
    class_named(c, name).filter(|&b| b != k)
}

/// A property of class `k` or a base of it.
fn class_property(c: &Cst, k: NodeId, name: &str) -> Option<Decl> {
    let mut cur = Some(k);
    let mut guard = 0;
    while let Some(x) = cur {
        if let Some(d) = find_in_items(c, x, name, DeclKind::Prop) {
            return Some(d);
        }
        guard += 1;
        if guard > 32 {
            break;
        }
        cur = base_class(c, x);
    }
    None
}

/// What `name`, used at node `at`, refers to.
fn resolve(c: &Cst, at: NodeId, name: &str) -> Option<Decl> {
    let mut cur = c.parent(at);
    while let Some(n) = cur {
        match c.kind(n) {
            SeqBlock | ParBlock | Body => {
                if let Some(d) = find_in_items(c, n, name, DeclKind::Local) {
                    return Some(d);
                }
            }
            LoopStmt => {
                if let Some(d) = find_in_items(c, n, name, DeclKind::Local) {
                    return Some(d);
                }
                if c.keyword(n).is_some_and(|t| c.text_of(t) == "foreach")
                    && c.toks(n).skip_while(|&t| c.text_of(t) != "[").any(|t| c.text_of(t) == name)
                {
                    return Some(Decl { kind: DeclKind::Local, item: n, dtype: None });
                }
            }
            FunctionDecl | TaskDecl => {
                if let Some(list) = c.tree.child(n, PortList) {
                    for p in kids(c, list) {
                        if c.kind(p) == Port {
                            if let Some(d) = declarators_named(c, p, name) {
                                return Some(Decl { kind: DeclKind::Arg, item: d, dtype: c.tree.child(p, DataType) });
                            }
                        }
                    }
                }
                // `function T C::f`: the body sees C's members.
                if let Some(nm) = c.tree.child(n, Name) {
                    let toks = c.tree.token_range(nm);
                    if toks.len() >= 3 && c.text_of(toks[toks.len() - 2]) == "::" {
                        if let Some(k) = class_named(c, c.text_of(toks[toks.len() - 3])) {
                            if let Some(d) = class_property(c, k, name) {
                                return Some(d);
                            }
                        }
                    }
                }
            }
            ClassDecl => {
                if let Some(d) = class_property(c, n, name) {
                    return Some(d);
                }
            }
            ModuleDecl | InterfaceDecl | ProgramDecl | PackageDecl | SourceFile => {
                if let Some(d) = find_in_items(c, n, name, DeclKind::Other) {
                    return Some(d);
                }
                if let Some(list) = c.tree.child(n, Header).and_then(|h| c.tree.child(h, PortList)) {
                    for p in kids(c, list) {
                        if c.kind(p) == Port {
                            if let Some(d) = declarators_named(c, p, name) {
                                return Some(Decl { kind: DeclKind::Other, item: d, dtype: c.tree.child(p, DataType) });
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        cur = c.parent(n);
    }
    None
}

/// The type name a data type spells (`pkt`, `p::pkt`, `pkt#(8)`), through
/// one typedef in the tree.
fn type_head<'t>(c: &Cst<'t>, dt: NodeId) -> Option<&'t str> {
    let toks = c.tree.token_range(dt);
    let end = toks.iter().position(|&t| c.text_of(t) == "#").unwrap_or(toks.len());
    let head = *toks[..end].last()?;
    is_ident(c, head).then(|| c.text_of(head))
}

fn typedef_named(c: &Cst, name: &str) -> Option<NodeId> {
    c.first_named(TypedefDecl, name)
}

/// The declared type is a class handle.
fn is_class_type(c: &Cst, d: &Decl) -> bool {
    let Some(dt) = d.dtype else { return false };
    if c.toks(dt).any(|t| c.text_of(t) == "virtual") || c.kind(d.item) != Declarator {
        return false;
    }
    if c.tree.child(d.item, Dim).is_some() {
        return false;
    }
    let mut name = match type_head(c, dt) {
        Some(n) => n,
        None => return false,
    };
    for _ in 0..4 {
        if class_named(c, name).is_some() {
            return true;
        }
        let Some(t) = typedef_named(c, name) else { return false };
        if c.tree.child(t, Dim).is_some() {
            return false;
        }
        match c.tree.child(t, DataType).and_then(|x| type_head(c, x)) {
            Some(n) if n != name => name = n,
            _ => return false,
        }
    }
    false
}

/// `getSymbolReference()` of an assignment's left side.
fn symbol_ref(c: &Cst, n: NodeId) -> Option<(Decl, String)> {
    let n = unparen(c, n);
    match c.kind(n) {
        NameExpr => {
            let t = c.first_tok(n)?;
            if !is_ident(c, t) {
                return None;
            }
            let name = c.text_of(t);
            resolve(c, n, name).map(|d| (d, name.to_string()))
        }
        SelectExpr => symbol_ref(c, kids(c, n).first().copied()?),
        MemberExpr => {
            let base = kids(c, n).first().copied()?;
            let member = last_tok(c, n)?;
            let mname = c.text_of(member).to_string();
            let b = unparen(c, base);
            if c.kind(b) == NameExpr && c.first_tok(b).is_some_and(|t| matches!(c.text_of(t), "this" | "super")) {
                let mut cur = c.parent(n);
                while let Some(x) = cur {
                    if c.kind(x) == ClassDecl {
                        let start = if c.text_of(c.first_tok(b)?) == "super" { base_class(c, x)? } else { x };
                        return class_property(c, start, &mname).map(|d| (d, mname));
                    }
                    if matches!(c.kind(x), FunctionDecl | TaskDecl) {
                        if let Some(nm) = c.tree.child(x, Name) {
                            let toks = c.tree.token_range(nm);
                            if toks.len() >= 3 && c.text_of(toks[toks.len() - 2]) == "::" {
                                let k = class_named(c, c.text_of(toks[toks.len() - 3]))?;
                                return class_property(c, k, &mname).map(|d| (d, mname));
                            }
                        }
                    }
                    cur = c.parent(x);
                }
                return None;
            }
            let (bd, _) = symbol_ref(c, b)?;
            if is_class_type(c, &bd) {
                // A member of a class handle: the property itself.
                let dt = bd.dtype?;
                let k = class_named(c, type_head(c, dt)?)
                    .or_else(|| {
                        let t = typedef_named(c, type_head(c, dt)?)?;
                        class_named(c, type_head(c, c.tree.child(t, DataType)?)?)
                    })?;
                return class_property(c, k, &mname).map(|d| (d, mname));
            }
            None
        }
        _ => None,
    }
}

fn assignment_op<'t>(c: &Cst<'t>, n: NodeId) -> &'t str {
    if c.kind(n) != BinaryExpr {
        return "";
    }
    c.toks(n).next().map(|t| c.text_of(t)).unwrap_or("")
}

fn in_class(c: &Cst, n: NodeId) -> bool {
    let mut cur = c.parent(n);
    while let Some(x) = cur {
        if c.kind(x) == ClassDecl {
            return true;
        }
        cur = c.parent(x);
    }
    false
}

/// Slang skips a body it rejects, so only a non-blocking assignment the
/// model kept (its body compiles) is reported; the model holds no module code.
fn ast_nba_on_property(d: &Design, csts: &[Cst], s: &mut Sink) {
    let kept: HashSet<(u32, u32, u32)> = d
        .classes()
        .iter()
        .flat_map(|k| k.methods.iter())
        .flat_map(|m| m.assignments.iter())
        .filter(|x| x.is_non_blocking)
        .map(|x| (x.loc.file, x.loc.line, x.loc.column))
        .collect();
    for c in csts {
        nba_on_property(c, &Ast::new(d, c), &kept, s);
    }
}

fn nba_on_property(c: &Cst, a: &Ast, kept: &HashSet<(u32, u32, u32)>, s: &mut Sink) {
    for n in c.nodes(BinaryExpr) {
        if assignment_op(c, n) != "<=" || !c.parent(n).is_some_and(|p| c.kind(p) == ExprStmt) {
            continue;
        }
        let Some(&lhs) = kids(c, n).first() else { continue };
        let Some((d, name)) = symbol_ref(c, lhs) else { continue };
        if d.kind != DeclKind::Prop {
            continue;
        }
        let Some(at) = c.first_tok(lhs) else { continue };
        let l = c.loc(at);
        if in_class(c, n) && !kept.contains(&(l.file, l.line, l.column)) {
            continue;
        }
        s.add(
            "SV-PRF-009",
            "NbaOnClassProperty",
            Severity::Warning,
            format!(
                "Non-blocking assignment ('<=') targeting class property '{name}'. \
                 Unlike static module/interface registers, class instances reside on the dynamic heap. \
                 Performing non-blocking assignments on class members forces the simulation kernel to allocate \
                 dynamic event descriptors in the NBA event queue, causing severe scheduler queue thrashing. \
                 Replace with blocking assignment ('=')."
            ),
            c.loc(at),
        );
        a.again(s, c, n);
    }
}

/// The declared type is a queue (`T q[$]`, or a typedef of one).
fn is_queue(c: &Cst, d: &Decl) -> bool {
    let first_dim = |x: NodeId| c.tree.child(x, Dim).map(|dim| c.tree.bare_text(dim));
    if c.kind(d.item) != Declarator {
        return false;
    }
    if let Some(dim) = first_dim(d.item) {
        return dim == "[$]" || dim.starts_with("[$:");
    }
    let Some(dt) = d.dtype else { return false };
    let Some(name) = type_head(c, dt) else { return false };
    typedef_named(c, name)
        .and_then(first_dim)
        .is_some_and(|dim| dim == "[$]" || dim.starts_with("[$:"))
}

fn queue_slice(c: &Cst, a: &Ast, s: &mut Sink) {
    for n in c.nodes(BinaryExpr) {
        if assignment_op(c, n) != "=" {
            continue;
        }
        let k = kids(c, n);
        let [lhs, rhs] = k[..] else { continue };
        // Cheap shape test first: symbol lookups are costly.
        let rhs = unparen(c, rhs);
        if c.kind(rhs) != SelectExpr || c.tok(rhs, ":").is_none() {
            continue;
        }
        let text = source_text(c, rhs);
        if !(text.contains("1:$") || text.contains("1 : $")) {
            continue;
        }
        let Some((d, name)) = symbol_ref(c, lhs) else { continue };
        if !is_queue(c, &d) {
            continue;
        }
        let Some(&value) = kids(c, rhs).first() else { continue };
        if symbol_ref(c, value).map(|x| x.0) != Some(d) {
            continue;
        }
        let Some(at) = c.first_tok(lhs) else { continue };
        s.add(
            "SV-PRF-008",
            "QueueSlicePopFirst",
            Severity::Warning,
            format!(
                "Queue slicing '{name} = {name}[1:$]' duplicates the entire remaining queue into newly allocated memory. \
                 Use in-place 'void'({name}.pop_front())' or 'item = {name}.pop_front()' for O(1) pointer adjustment."
            ),
            c.loc(at),
        );
        a.again(s, c, n);
    }
}
