//! Syntax rules, fourth batch (builder slice N6.3): conditions and loops.
//! See [`crate::syntax`].

use std::collections::HashSet;

use sv_syntax::{Child, NodeId};

use crate::syntax::Cst;
use crate::Check::Syntax;
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-COR-005", name: "NonShortCircuitLogical", check: Syntax(non_short_circuit) },
        Rule { id: "SV-COR-007", name: "AssignmentInExpression", check: Syntax(assignment_in_condition) },
        Rule { id: "SV-COR-011", name: "ModifyingLoopIndexInsideFor", check: Syntax(loop_index_modified) },
        Rule { id: "SV-CON-012", name: "ForkLoopVariableCapture", check: Syntax(fork_loop_capture) },
        Rule { id: "SV-PRF-010", name: "StringConcatInLoop", check: Syntax(concat_in_loop) },
    ]
}

fn op_text<'t>(c: &Cst<'t>, n: NodeId) -> &'t str {
    c.toks(n).next().map_or("", |t| c.text_of(t))
}

fn kids(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.tree.children(n).collect()
}

fn keyword_is(c: &Cst, n: NodeId, kw: &str) -> bool {
    c.keyword(n).is_some_and(|k| c.text_of(k) == kw)
}

/// A `for` loop's initializers, stop expression and body.
fn for_parts(c: &Cst, l: NodeId) -> (Vec<NodeId>, Option<NodeId>, Option<NodeId>) {
    let mut semis = 0;
    let (mut init, mut stop) = (Vec::new(), None);
    for k in c.kids(l) {
        match *k {
            Child::Token(t) if c.text_of(t) == ";" => semis += 1,
            Child::Node(n) if semis == 0 => init.push(n),
            Child::Node(n) if semis == 1 => stop = Some(n),
            _ => {}
        }
    }
    (init, stop, kids(c, l).last().copied())
}

/// Every node under `n` (itself included), in source order.
fn subtree(c: &Cst, n: NodeId, out: &mut Vec<NodeId>) {
    out.push(n);
    for k in c.tree.children(n) {
        subtree(c, k, out);
    }
}

/// The predicates slang keeps as a `ConditionalPredicate`: an `if`
/// condition and a `?:` condition.
fn predicates(c: &Cst) -> Vec<NodeId> {
    let mut out: Vec<NodeId> = c.nodes(IfStmt).into_iter().filter_map(|n| kids(c, n).first().copied()).collect();
    out.extend(c.nodes(CondExpr).into_iter().filter_map(|n| kids(c, n).first().copied()));
    out
}

const BOOLEAN_OPS: &[&str] = &["==", "!=", "===", "!==", "==?", "!=?", "<", "<=", ">", ">=", "&&", "||", "->", "<->"];

fn is_boolean(c: &Cst, n: NodeId) -> bool {
    c.kind(n) == BinaryExpr && BOOLEAN_OPS.contains(&op_text(c, n))
}

fn non_short_circuit(c: &Cst, s: &mut Sink) {
    fn check(c: &Cst, e: NodeId, s: &mut Sink) {
        match c.kind(e) {
            BinaryExpr if matches!(op_text(c, e), "&" | "|") => {
                let k = kids(c, e);
                let boolean = k.iter().any(|&x| {
                    is_boolean(c, x)
                        || (c.kind(x) == ParenExpr && kids(c, x).first().is_some_and(|&i| is_boolean(c, i)))
                });
                if boolean {
                    let op = op_text(c, e);
                    let rep = if op == "&" { "&&" } else { "||" };
                    if let Some(t) = c.toks(e).next() {
                        s.add(
                            "SV-COR-005",
                            "NonShortCircuitLogical",
                            Severity::Warning,
                            format!(
                                "Bitwise operator '{op}' used in conditional predicate where logical '{rep}' is expected. Bitwise operators do not short-circuit, unconditionally evaluating the right-hand side and risking null-pointer dereferences or index-out-of-bounds faults. Replace with '{rep}'."
                            ),
                            c.loc(t),
                        );
                    }
                }
                for x in k {
                    check(c, x, s);
                }
            }
            ParenExpr => {
                if let Some(&i) = kids(c, e).first() {
                    check(c, i, s);
                }
            }
            _ => {}
        }
    }
    for p in predicates(c) {
        check(c, p, s);
    }
}

fn assignment_in_condition(c: &Cst, s: &mut Sink) {
    let mut roots = predicates(c);
    for l in c.nodes(LoopStmt) {
        if keyword_is(c, l, "while") || keyword_is(c, l, "repeat") {
            roots.extend(kids(c, l).first().copied());
        } else if keyword_is(c, l, "for") {
            roots.extend(for_parts(c, l).1);
        }
    }
    for l in c.nodes(DoWhileStmt) {
        let k = kids(c, l);
        if k.len() >= 2 {
            roots.push(k[k.len() - 1]);
        }
    }
    let mut found = Vec::new();
    for r in roots {
        let mut all = Vec::new();
        subtree(c, r, &mut all);
        for n in all {
            if c.kind(n) == BinaryExpr && matches!(op_text(c, n), "=" | "+=" | "-=" | "*=" | "/=") {
                found.push(n);
            }
        }
    }
    found.sort_by_key(|&n| c.first_tok(n));
    for n in found {
        let Some(t) = c.toks(n).next() else { continue };
        s.add(
            "SV-COR-007",
            "AssignmentInExpression",
            Severity::Warning,
            format!(
                "Assignment operator '{}' used inside conditional predicate. In SystemVerilog, an assignment assigns the value and evaluates its truthiness, which is frequently an unintended typo for the equality operator '=='.",
                c.text_of(t)
            ),
            c.loc(t),
        );
    }
}

/// Names a `for` loop declares or assigns in its initializers.
fn for_vars(c: &Cst, init: &[NodeId], first_token: bool) -> Vec<String> {
    let mut vars = Vec::new();
    for &i in init {
        match c.kind(i) {
            DataDecl => {
                for d in kids(c, i).into_iter().filter(|&d| c.kind(d) == Declarator) {
                    if let Some(nm) = c.tree.child(d, Name) {
                        vars.push(c.tree.bare_text(nm));
                    }
                }
            }
            BinaryExpr if op_text(c, i) == "=" => {
                let Some(&l) = kids(c, i).first() else { continue };
                if first_token {
                    if let Some(t) = c.first_tok(l) {
                        vars.push(c.text_of(t).to_string());
                    }
                } else if is_ident_name(c, l) {
                    vars.push(c.tree.bare_text(l));
                }
            }
            _ => {}
        }
    }
    vars
}

/// slang's `IdentifierName`: a plain name.
fn is_ident_name(c: &Cst, n: NodeId) -> bool {
    c.kind(n) == NameExpr
        && c.tree.token_bounds(n).is_some_and(|(a, b)| {
            a == b && {
                let t = c.text_of(a);
                t.starts_with(|ch: char| ch.is_ascii_alphabetic() || ch == '_')
                    && !matches!(t, "this" | "super" | "null" | "local")
            }
        })
}

fn loop_index_modified(c: &Cst, s: &mut Sink) {
    fn walk(c: &Cst, n: NodeId, vars: &HashSet<String>, s: &mut Sink) {
        let target = match c.kind(n) {
            PostfixExpr | PrefixExpr if matches!(op_text(c, n), "++" | "--") => {
                kids(c, n).first().copied().zip(c.first_tok(n))
            }
            BinaryExpr if matches!(op_text(c, n), "=" | "+=" | "-=") => {
                kids(c, n).first().copied().zip(c.toks(n).next())
            }
            _ => None,
        };
        if let Some((e, at)) = target {
            if is_ident_name(c, e) {
                let name = c.tree.bare_text(e);
                if vars.contains(&name) {
                    s.add(
                        "SV-COR-011",
                        "ModifyingLoopIndexInsideFor",
                        Severity::Warning,
                        format!(
                            "Loop control variable '{name}' is procedurally modified inside the body of the 'for' loop. \
                             Altering the loop index within the loop body conflicts with the loop iteration step logic, \
                             easily causing infinite loops, skipped elements, or subtle indexing off-by-one errors. \
                             Move index adjustments into the loop update clause or use a 'while' loop."
                        ),
                        c.loc(at),
                    );
                }
            }
        }
        if c.kind(n) == LoopStmt && keyword_is(c, n, "for") {
            return;
        }
        for k in c.tree.children(n) {
            walk(c, k, vars, s);
        }
    }
    for l in c.nodes(LoopStmt).into_iter().filter(|&l| keyword_is(c, l, "for")) {
        let (init, _, body) = for_parts(c, l);
        let vars: HashSet<String> = for_vars(c, &init, false).into_iter().collect();
        if let (false, Some(body)) = (vars.is_empty(), body) {
            walk(c, body, &vars, s);
        }
    }
}

/// The names slang reads as `IdentifierName`s under `n`, in order, with
/// the names `DataDeclaration`s declared before each: plain names, member
/// names and user type names, but not the name before a select.
fn ident_names(c: &Cst, n: NodeId, locals: &mut HashSet<String>, out: &mut HashSet<String>) {
    let select_base =
        |x: NodeId| c.parent(x).is_some_and(|p| c.kind(p) == SelectExpr && kids(c, p).first() == Some(&x));
    match c.kind(n) {
        DataDecl => {
            for d in kids(c, n).into_iter().filter(|&d| c.kind(d) == Declarator) {
                if let Some(nm) = c.tree.child(d, Name) {
                    locals.insert(c.tree.bare_text(nm));
                }
            }
        }
        NameExpr if is_ident_name(c, n) && !select_base(n) => {
            let name = c.tree.bare_text(n);
            if !locals.contains(&name) {
                out.insert(name);
            }
        }
        MemberExpr if !select_base(n) => {
            if let Some((_, b)) = c.tree.token_bounds(n) {
                let name = c.text_of(b).to_string();
                if !locals.contains(&name) {
                    out.insert(name);
                }
            }
        }
        DataType => {
            if let Some((a, b)) = c.tree.token_bounds(n) {
                let t = c.text_of(a);
                if a == b && t.starts_with(|ch: char| ch.is_ascii_alphabetic() || ch == '_') && !locals.contains(t) {
                    out.insert(t.to_string());
                }
            }
        }
        _ => {}
    }
    for k in c.tree.children(n) {
        ident_names(c, k, locals, out);
    }
}

fn fork_loop_capture(c: &Cst, s: &mut Sink) {
    let mut loops = Vec::new();
    for l in c.nodes(LoopStmt) {
        let vars: Vec<String> = if keyword_is(c, l, "for") {
            let (init, _, _) = for_parts(c, l);
            for_vars(c, &init, true)
        } else if keyword_is(c, l, "foreach") {
            let toks: Vec<u32> = c.toks(l).collect();
            let open = toks.iter().position(|&t| c.text_of(t) == "[");
            open.map(|o| {
                toks[o..]
                    .iter()
                    .map(|&t| c.text_of(t))
                    .filter(|t| t.starts_with(|ch: char| ch.is_ascii_alphabetic() || ch == '_'))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
        } else {
            continue;
        };
        if let (false, Some(&body)) = (vars.is_empty(), kids(c, l).last()) {
            loops.push((vars, body));
        }
    }
    for (vars, body) in loops {
        let mut all = Vec::new();
        subtree(c, body, &mut all);
        for b in all.into_iter().filter(|&b| c.kind(b) == ParBlock) {
            let join = c.last_text(b);
            if join != "join_none" && join != "join_any" {
                continue;
            }
            let mut ids = HashSet::new();
            ident_names(c, b, &mut HashSet::new(), &mut ids);
            // The C++ rule walks a std::unordered_set, which for a few names in
            // distinct buckets yields the last inserted first.
            let Some(var) = vars.iter().rev().find(|v| ids.contains(*v)) else { continue };
            let Some(t) = c.first_tok(b) else { continue };
            s.add(
                "SV-CON-012",
                "ForkLoopVariableCapture",
                Severity::Error,
                format!(
                    "Loop variable '{var}' is referenced directly inside a 'fork ... {join}' child thread without an isolating local variable copy. \
                     In SystemVerilog (IEEE 1800 § 9.3.2), threads spawned \
                     with join_none or join_any execute after the loop advances, causing all spawned child threads to read \
                     the final mutated value of the loop variable. Declare an automatic local variable copy \
                     (e.g. 'automatic int _{var} = {var};') inside the fork block to bind the per-iteration value."
                ),
                c.loc(t),
            );
        }
    }
}

fn concat_in_loop(c: &Cst, s: &mut Sink) {
    let mut loops = c.nodes(LoopStmt);
    loops.extend(c.nodes(DoWhileStmt));
    loops.sort_by_key(|&n| c.first_tok(n));
    for l in loops {
        let body = match c.keyword(l).map(|k| c.text_of(k)) {
            Some("for" | "foreach" | "while" | "repeat") => kids(c, l).last().copied(),
            Some("do") => kids(c, l).first().copied(),
            _ => None,
        };
        let Some(body) = body else { continue };
        let mut all = Vec::new();
        subtree(c, body, &mut all);
        for n in all {
            if c.kind(n) != BinaryExpr || op_text(c, n) != "=" {
                continue;
            }
            let k = kids(c, n);
            let [left, right] = k[..] else { continue };
            let var = c.last_text(left);
            let Some(op) = c.toks(n).next() else { continue };
            let msg = match c.kind(right) {
                ConcatExpr if is_plain_concat(c, right) && c.text(right).contains(var) => format!(
                    "Iterative concatenation of variable '{var}' inside a loop construct. In SystemVerilog, repeatedly concatenating strings or arrays inside loops causes quadratic heap allocations and memory copies (O(N^2)). Accumulate elements into a queue ('q.push_back(...)') or pre-allocate fixed buffers instead."
                ),
                CallExpr
                    if kids(c, right).first().is_some_and(|&f| c.last_text(f) == "$sformatf")
                        && c.text(right).contains(var) =>
                {
                    format!(
                        "Iterative string formatting into '{var}' via $sformatf inside a loop construct. Repeatedly reallocating and copying strings inside loops causes severe memory fragmentation and simulation slowdown. Accumulate messages into a queue or use stream buffers instead."
                    )
                }
                _ => continue,
            };
            s.add("SV-PRF-010", "StringConcatInLoop", Severity::Warning, msg, c.loc(op));
        }
    }
}

/// slang's `ConcatenationExpression`: not `{}`, a replication or a stream.
fn is_plain_concat(c: &Cst, n: NodeId) -> bool {
    let k = kids(c, n);
    let replication = k.len() == 2 && c.tok(n, ",").is_none() && c.kind(k[1]) == ConcatExpr;
    !k.is_empty() && !replication && c.tok(n, "<<").is_none() && c.tok(n, ">>").is_none()
}
