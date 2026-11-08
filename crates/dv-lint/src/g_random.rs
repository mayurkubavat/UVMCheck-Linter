//! Group E (builder slice N6.3): randomization rules. Syntax ports of the
//! C++ rules under `cpp/src/rules/random/`, and of the ones there that walk
//! slang's AST where the syntax tree carries the same facts. Messages are
//! verbatim. See [`crate::syntax`] for how sv-syntax shapes stand for
//! slang's.

use std::collections::{HashMap, HashSet};

use sv_syntax::{Child, NodeId};

use crate::syntax::Cst;
use crate::Check::Syntax;
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-RND-001", name: "UncheckedRandomize", check: Syntax(unchecked_randomize) },
        Rule { id: "SV-RND-004", name: "InlineShadowNoLocal", check: Syntax(inline_shadow_no_local) },
        Rule { id: "SV-RND-005", name: "NonRandFieldConstrained", check: Syntax(non_rand_field_constrained) },
        Rule { id: "SV-RND-008", name: "NonInvertibleConstraintOp", check: Syntax(non_invertible_op) },
        Rule { id: "SV-RND-010", name: "CyclicSolveBefore", check: Syntax(cyclic_solve_before) },
        Rule { id: "SV-RND-011", name: "RedundantConstraint", check: Syntax(redundant_constraint) },
        Rule { id: "SV-RND-013", name: "UnrestoredRandMode", check: Syntax(unrestored_rand_mode) },
        Rule { id: "SV-RND-015", name: "UnconstrainedRandVariable", check: Syntax(unconstrained_rand) },
        Rule { id: "SV-RND-016", name: "ConcurrentRandomizeInFork", check: Syntax(concurrent_randomize) },
        Rule { id: "SV-RND-020", name: "RandcInDynamicContainer", check: Syntax(randc_dynamic) },
        Rule { id: "SV-RND-022", name: "ConstraintCallingNonPureFunction", check: Syntax(non_pure_in_constraint) },
        Rule { id: "SV-RND-024", name: "RandomizeInsideAssert", check: Syntax(randomize_in_assert) },
    ]
}

// ---- helpers shared by the group E files ----

pub(crate) fn kids(c: &Cst, n: NodeId) -> Vec<NodeId> {
    c.tree.children(n).collect()
}

/// The first direct token's text ("" when none): a binary operator, a
/// statement keyword.
pub(crate) fn op<'t>(c: &Cst<'t>, n: NodeId) -> &'t str {
    c.toks(n).next().map(|t| c.text_of(t)).unwrap_or("")
}

/// Direct tokens' texts.
pub(crate) fn own<'t>(c: &Cst<'t>, n: NodeId) -> Vec<&'t str> {
    c.toks(n).map(|t| c.text_of(t)).collect()
}

/// `n` and every node below it, in pre-order.
pub(crate) fn subtree(c: &Cst, n: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut stack = vec![n];
    while let Some(x) = stack.pop() {
        out.push(x);
        let k: Vec<NodeId> = c.tree.children(x).collect();
        stack.extend(k.into_iter().rev());
    }
    out
}

/// Trims what C++'s `find_first_not_of(" \t\r\n")` trims.
pub(crate) fn trim(s: &str) -> &str {
    s.trim_matches([' ', '\t', '\r', '\n'])
}

/// A declaration's name: the last token of its Name child.
pub(crate) fn decl_name(c: &Cst, n: NodeId) -> Option<(String, u32)> {
    let name = c.tree.child(n, Name)?;
    let (_, last) = c.tree.token_bounds(name)?;
    Some((c.text_of(last).to_string(), last))
}

/// A class declaration with a parameter port list (a generic class, which
/// the UVM model does not index).
pub(crate) fn is_parameterized(c: &Cst, cls: NodeId) -> bool {
    c.tree
        .child(cls, Header)
        .and_then(|h| c.tree.child(h, ParamList))
        .is_some_and(|p| c.tree.children(p).next().is_some())
}

/// An out-of-block class declaration name such as `pc::inner` is not a
/// class of its own here.
pub(crate) fn class_decls(c: &Cst) -> Vec<NodeId> {
    c.nodes(ClassDecl)
}

/// The class's direct items (properties, methods, constraints).
pub(crate) fn class_items(c: &Cst, cls: NodeId) -> Vec<NodeId> {
    c.tree
        .children(cls)
        .filter(|&x| !matches!(c.kind(x), Name | Header | EndLabel | Attribute))
        .collect()
}

/// A data declaration's qualifier tokens (before its type).
pub(crate) fn qualifiers<'t>(c: &Cst<'t>, decl: NodeId) -> Vec<&'t str> {
    let mut out = Vec::new();
    for k in c.kids(decl) {
        match *k {
            Child::Token(t) => out.push(c.text_of(t)),
            Child::Node(_) => break,
        }
    }
    out
}

/// Declarators of a declaration: their nodes and name tokens.
pub(crate) fn declarators(c: &Cst, decl: NodeId) -> Vec<(NodeId, String, u32)> {
    c.tree
        .children(decl)
        .filter(|&d| c.kind(d) == Declarator)
        .filter_map(|d| {
            let (n, t) = decl_name(c, d)?;
            Some((d, n, t))
        })
        .collect()
}

/// slang's `stripConversions`: casts (and parentheses, which the AST has
/// none of).
pub(crate) fn strip_casts(c: &Cst, mut e: NodeId) -> NodeId {
    loop {
        match c.kind(e) {
            ParenExpr => match c.tree.children(e).next() {
                Some(x) => e = x,
                None => return e,
            },
            CastExpr => match c.tree.children(e).nth(1) {
                Some(x) => e = x,
                None => return e,
            },
            _ => return e,
        }
    }
}

/// A call's invocation node and method name, as slang's
/// `getSubroutineName()`: `f()`, `o.f()`, `p::f()`, `o.f() with {...}`.
pub(crate) fn call_name<'t>(c: &Cst<'t>, call: NodeId) -> Option<&'t str> {
    if c.kind(call) != CallExpr {
        return None;
    }
    let callee = c.tree.children(call).next()?;
    match c.kind(callee) {
        CallExpr => call_name(c, callee),
        NameExpr | MemberExpr | ScopedExpr => Some(c.last_text(callee)),
        _ => None,
    }
}

/// The `with` clause of a call, if any.
pub(crate) fn with_clause(c: &Cst, call: NodeId) -> Option<NodeId> {
    c.tree.child(call, WithClause)
}

/// Code swe::UserCodeVisitor walks: not in package `uvm_pkg`, not in a
/// class named `uvm_*`, not from uvm_pkg.sv or uvm_mock.svh.
pub(crate) fn user_code(c: &Cst, n: NodeId) -> bool {
    let mut x = n;
    loop {
        match c.kind(x) {
            PackageDecl if decl_name(c, x).is_some_and(|(s, _)| s == "uvm_pkg") => return false,
            ClassDecl if decl_name(c, x).is_some_and(|(s, _)| s.starts_with("uvm_")) => return false,
            _ => {}
        }
        match c.parent(x) {
            Some(p) => x = p,
            None => break,
        }
    }
    true
}

/// Text between two tokens as written (their trivia between, not before).
pub(crate) fn span_text(c: &Cst, a: u32, b: u32) -> String {
    let mut s = String::new();
    for t in a..=b {
        let tok = &c.tree.tokens[t as usize];
        if t != a && !tok.from_macro {
            s.push_str(c.tree.trivia(t));
        }
        s.push_str(&tok.text);
    }
    s
}

// ---- SV-RND-001 ----

fn unchecked_randomize(c: &Cst, s: &mut Sink) {
    for st in c.nodes(ExprStmt) {
        let Some(e) = c.tree.children(st).next() else { continue };
        let e = strip_casts(c, e);
        if call_name(c, e) != Some("randomize") || !crate::g_random_sema::ast_ok(c, e) {
            continue;
        }
        let Some(at) = c.first_tok(e) else { continue };
        s.add(
            "SV-RND-001",
            "UncheckedRandomize",
            Severity::Error,
            "Return value of 'randomize()' is discarded or cast to void. If constraint solving fails, \
             simulation proceeds silently with corrupt, unconstrained, or stale transaction data. \
             Check the return value with 'if (!obj.randomize()) `uvm_fatal(...)'. Avoid 'assert(obj.randomize())': turning assertions off skips the call (SV-RND-024)."
                .into(),
            c.loc(at),
        );
    }
}

// ---- SV-RND-004 ----

fn inline_shadow_no_local(c: &Cst, s: &mut Sink) {
    for call in c.nodes(CallExpr) {
        let Some(w) = with_clause(c, call) else { continue };
        let Some(block) = c.tree.child(w, ConstraintBlock) else { continue };
        let Some(method) = c.tree.children(call).next() else { continue };
        let method_str = c.text(method);
        if !method_str.contains("randomize") {
            continue;
        }
        let target = method_str.find('.').map(|i| trim(&method_str[..i]).to_string()).unwrap_or_default();
        for n in subtree(c, block) {
            if c.kind(n) != BinaryExpr || !matches!(op(c, n), "==" | "==?" | "===") {
                continue;
            }
            let k = kids(c, n);
            let (Some(&l), Some(&r)) = (k.first(), k.get(1)) else { continue };
            let (ls, rs) = (c.trimmed(l), c.trimmed(r));
            if ls.is_empty() || ls != rs || ls.contains("local::") || ls.contains('.') {
                continue;
            }
            let prefix = if target.is_empty() { "item" } else { &target };
            let Some(at) = c.first_tok(n) else { continue };
            s.add(
                "SV-RND-004",
                "InlineShadowNoLocal",
                Severity::Error,
                format!(
                    "Inline constraint '{ls} == {rs}' in 'randomize() with' shadows \
                     the local scope variable without the 'local::' qualifier (IEEE 1800 § 18.7). Object properties \
                     take precedence in inline constraints, causing '{ls} == {rs}' to evaluate \
                     as a tautology on '{prefix}.{ls}' and leaving the field unconstrained. \
                     Qualify the local variable with 'local::{ls}'."
                ),
                c.loc(at),
            );
        }
    }
}

// ---- SV-RND-005 ----

/// Classes the UVM model indexes (not generic ones), with their items.
pub(crate) fn model_classes(c: &Cst) -> Vec<(NodeId, String)> {
    class_decls(c)
        .into_iter()
        .filter(|&k| !is_parameterized(c, k))
        .filter_map(|k| {
            let (name, _) = decl_name(c, k)?;
            // `class pc::inner` out of block names no class of its own.
            if c.tree.child(k, Header).is_some_and(|h| c.tok(h, "::").is_some()) {
                return None;
            }
            Some((k, name))
        })
        .collect()
}

fn non_rand_field_constrained(c: &Cst, s: &mut Sink) {
    for (cls, cname) in model_classes(c) {
        let items = class_items(c, cls);
        let mut props: Vec<(String, u32)> = Vec::new();
        for &it in &items {
            if c.kind(it) != DataDecl || c.tree.child(it, DataType).is_none() {
                continue;
            }
            let q = qualifiers(c, it);
            if q.iter().any(|t| matches!(*t, "rand" | "randc" | "static" | "typedef")) {
                continue;
            }
            for (_, n, t) in declarators(c, it) {
                if !props.iter().any(|(p, _)| *p == n) {
                    props.push((n, t));
                }
            }
        }
        if props.is_empty() {
            continue;
        }
        for &it in &items {
            if c.kind(it) != ConstraintDecl {
                continue;
            }
            let Some((block_name, _)) = decl_name(c, it) else { continue };
            let text = c.text(it);
            for (p, at) in &props {
                let hit = [" inside", " ==", " <", " >", " <=", " >="]
                    .iter()
                    .any(|suffix| text.contains(&format!("{p}{suffix}")));
                if hit {
                    s.add(
                        "SV-RND-005",
                        "NonRandFieldConstrained",
                        Severity::Error,
                        format!(
                            "Class property '{p}' in class '{cname}' is constrained in constraint block '{block_name}' but is not declared with the 'rand' or 'randc' keyword. In SystemVerilog \
                             (IEEE 1800 § 18.4), non-rand fields are treated as fixed constants by the solver \
                             and will never be randomized. Add 'rand' to the declaration of '{p}'."
                        ),
                        c.loc(*at),
                    );
                }
            }
        }
    }
}

// ---- SV-RND-008 ----

fn non_invertible_op(c: &Cst, s: &mut Sink) {
    for (cls, _) in model_classes(c) {
        for it in class_items(c, cls) {
            if c.kind(it) != ConstraintDecl {
                continue;
            }
            let Some((block, _)) = decl_name(c, it) else { continue };
            for n in subtree(c, it) {
                if c.kind(n) != BinaryExpr {
                    continue;
                }
                let what = match op(c, n) {
                    "%" => "% (modulo)",
                    "/" => "/ (division)",
                    "**" => "** (exponentiation)",
                    _ => continue,
                };
                let Some(at) = c.first_tok(n) else { continue };
                s.add(
                    "SV-RND-008",
                    "NonInvertibleConstraintOp",
                    Severity::Warning,
                    format!(
                        "Non-invertible mathematical operator '{what}' used inside constraint block '{block}'. Non-linear arithmetic cannot be mapped efficiently to 2-state SAT/SMT solver equations, triggering exponential backtracking and simulator stalls."
                    ),
                    c.loc(at),
                );
            }
        }
    }
}

// ---- SV-RND-010 ----

/// A solve-before item: its `solve` token and the two lists of expressions
/// (each as written, trimmed, with its tokens).
fn solve_before(c: &Cst, item: NodeId) -> Option<(u32, Vec<String>, Vec<String>)> {
    let toks: Vec<u32> = c.toks(item).collect();
    let first = *toks.first()?;
    if c.text_of(first) != "solve" {
        return None;
    }
    let mut lists: Vec<Vec<String>> = vec![Vec::new(), Vec::new()];
    let mut which = 0;
    let mut start: Option<u32> = None;
    let mut depth = 0i32;
    let flush = |lists: &mut Vec<Vec<String>>, which: usize, start: &mut Option<u32>, end: u32| {
        if let Some(a) = start.take() {
            lists[which].push(trim(&span_text(c, a, end)).to_string());
        }
    };
    let mut prev = first;
    for &t in &toks[1..] {
        let x = c.text_of(t);
        match x {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => depth -= 1,
            _ => {}
        }
        if depth == 0 && (x == "before" || x == "," || x == ";") {
            flush(&mut lists, which, &mut start, prev);
            if x == "before" {
                which = 1;
            }
        } else if start.is_none() {
            start = Some(t);
        }
        prev = t;
    }
    let after = lists.pop()?;
    let before = lists.pop()?;
    Some((first, before, after))
}

fn cyclic_solve_before(c: &Cst, s: &mut Sink) {
    for cls in class_decls(c) {
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        let has_path = |adj: &HashMap<String, Vec<String>>, start: &str, target: &str| -> bool {
            if start == target {
                return true;
            }
            let mut q = std::collections::VecDeque::from([start.to_string()]);
            let mut seen: HashSet<String> = HashSet::from([start.to_string()]);
            while let Some(cur) = q.pop_front() {
                if let Some(next) = adj.get(&cur) {
                    for n in next {
                        if n == target {
                            return true;
                        }
                        if seen.insert(n.clone()) {
                            q.push_back(n.clone());
                        }
                    }
                }
            }
            false
        };
        for n in subtree(c, cls) {
            if c.kind(n) != ConstraintItem {
                continue;
            }
            let Some((kw, before, after)) = solve_before(c, n) else { continue };
            for u in &before {
                for v in &after {
                    if u.is_empty() || v.is_empty() {
                        continue;
                    }
                    if u == v || has_path(&adj, v, u) {
                        s.add(
                            "SV-RND-010",
                            "CyclicSolveBefore",
                            Severity::Error,
                            format!(
                                "Cyclic dependency in solve...before order involving variables '{u}' and '{v}'. Cyclic solve-before constraints form an unsatisfiable dependency graph per IEEE 1800 § 18.5.10."
                            ),
                            c.loc(kw),
                        );
                    } else {
                        adj.entry(u.clone()).or_default().push(v.clone());
                    }
                }
            }
        }
    }
}

// ---- SV-RND-011 ----

/// slang's `ExpressionConstraintSyntax`: `[soft] expr ;` (an expression or
/// a dist), with its expression.
pub(crate) fn expression_constraint(c: &Cst, item: NodeId) -> Option<(bool, NodeId)> {
    let o = own(c, item);
    let soft = match o.as_slice() {
        [";"] => false,
        ["soft", ";"] => true,
        _ => return None,
    };
    let k = kids(c, item);
    match k.as_slice() {
        [e] if c.kind(*e) != ConstraintBlock && c.kind(*e) != ConstraintItem => Some((soft, *e)),
        _ => None,
    }
}

fn strip_comments_and_ws(input: &str) -> String {
    let b = input.as_bytes();
    let mut out = Vec::new();
    let (mut line, mut block) = (false, false);
    let mut i = 0;
    while i < b.len() {
        let ch = b[i];
        if line {
            if ch == b'\n' {
                line = false;
            }
        } else if block {
            if ch == b'*' && b.get(i + 1) == Some(&b'/') {
                block = false;
                i += 1;
            }
        } else if ch == b'/' && b.get(i + 1) == Some(&b'/') {
            line = true;
            i += 1;
        } else if ch == b'/' && b.get(i + 1) == Some(&b'*') {
            block = true;
            i += 1;
        } else if !matches!(ch, b' ' | b'\t' | b'\n' | b'\r') {
            out.push(ch);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn redundant_constraint(c: &Cst, s: &mut Sink) {
    for cls in class_decls(c) {
        let cname = decl_name(c, cls).map(|x| x.0).filter(|x| !x.is_empty()).unwrap_or_else(|| "unnamed".into());
        let mut seen: HashSet<String> = HashSet::new();
        for item in subtree(c, cls) {
            if c.kind(item) != ConstraintItem {
                continue;
            }
            let Some((soft, e)) = expression_constraint(c, item) else { continue };
            let raw = trim(&c.text(e)).to_string();
            let mut clean = raw.as_str();
            if let Some(i) = clean.rfind('\n') {
                clean = clean[i + 1..].trim_start_matches([' ', '\t', '\n', '\r']);
            }
            let key = format!("{}{}", if soft { "soft:" } else { "" }, strip_comments_and_ws(&raw));
            if !seen.insert(key) {
                let Some(at) = c.first_tok(item) else { continue };
                s.add(
                    "SV-RND-011",
                    "RedundantConstraint",
                    Severity::Warning,
                    format!(
                        "Duplicate constraint expression '{clean}' in class '{cname}'. Duplicate constraints bloat the solver constraint graph and increase solver compile overhead without introducing new assertions."
                    ),
                    c.loc(at),
                );
            }
        }
    }
}

// ---- SV-RND-013 ----

fn unrestored_rand_mode(c: &Cst, s: &mut Sink) {
    for f in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)) {
        let Some(body) = c.tree.child(f, Body) else { continue };
        let routine = c.tree.child(f, Name).map(|n| c.last_text(n).to_string()).unwrap_or_default();
        let mut disabled: Option<u32> = None;
        let mut restored = false;
        let mut method = "";
        for item in c.items(body) {
            let text = c.text(item);
            let first = c.first_tok(item);
            if text.contains("rand_mode(0)") || text.contains("rand_mode(1'b0)") {
                disabled = first.or(disabled);
                method = "rand_mode";
            } else if text.contains("rand_mode(1)") || text.contains("rand_mode(1'b1)") {
                restored = true;
            }
            if text.contains("constraint_mode(0)") || text.contains("constraint_mode(1'b0)") {
                disabled = first.or(disabled);
                method = "constraint_mode";
            } else if text.contains("constraint_mode(1)") || text.contains("constraint_mode(1'b1)") {
                restored = true;
            }
        }
        let Some(at) = disabled.filter(|_| !restored) else { continue };
        s.add(
            "SV-RND-013",
            "UnrestoredRandMode",
            Severity::Warning,
            format!(
                "Random mode or constraint was disabled via '.{method}(0)' in '{routine}()' but was never restored with '.{method}(1)'. Permanently leaving random fields or constraints disabled causes silent stimulus degradation across subsequent transactions."
            ),
            c.loc(at),
        );
    }
}

// ---- SV-RND-015 ----

/// Whether a name chain (slang's ScopedName / IdentifierSelectName
/// pieces) starts here: names, selects of names, members of names.
fn is_name_chain(c: &Cst, n: NodeId) -> bool {
    match c.kind(n) {
        NameExpr | ScopedExpr => true,
        SelectExpr | MemberExpr => c.tree.children(n).next().is_some_and(|b| is_name_chain(c, b)),
        _ => false,
    }
}

fn is_identifier(t: &str) -> bool {
    let mut ch = t.chars();
    ch.next().is_some_and(|f| f.is_ascii_alphabetic() || f == '_' || f == '\\')
        && !matches!(t, "this" | "super" | "null" | "local" | "std" | "before" | "solve" | "foreach" | "inside" | "dist" | "soft" | "if" | "else" | "unique")
}

/// slang's `IdentifierNameSyntax` identifiers below a node.
fn identifier_names(c: &Cst, root: NodeId, out: &mut HashSet<String>) {
    for n in subtree(c, root) {
        match c.kind(n) {
            NameExpr => {
                let o = own(c, n);
                let [t] = o.as_slice() else { continue };
                if !is_identifier(t) {
                    continue;
                }
                // `q[i]` is an IdentifierSelectName.
                let selected = c
                    .parent(n)
                    .is_some_and(|p| c.kind(p) == SelectExpr && c.tree.children(p).next() == Some(n));
                if !selected {
                    out.insert(t.to_string());
                }
            }
            MemberExpr | ScopedExpr => {
                // The member of a name chain is an IdentifierName unless
                // it is selected itself (`a.b[0]`).
                let base_ok = c.kind(n) == ScopedExpr
                    || c.tree.children(n).next().is_some_and(|b| is_name_chain(c, b));
                let selected = c
                    .parent(n)
                    .is_some_and(|p| c.kind(p) == SelectExpr && c.tree.children(p).next() == Some(n));
                if base_ok && !selected {
                    if let Some(t) = c.toks(n).last() {
                        let x = c.text_of(t);
                        if is_identifier(x) {
                            out.insert(x.to_string());
                        }
                    }
                }
            }
            ConstraintItem => {
                // Names written as tokens: solve-before lists and foreach
                // headers.
                let toks: Vec<u32> = c.toks(n).collect();
                for (i, &t) in toks.iter().enumerate() {
                    let x = c.text_of(t);
                    if !is_identifier(x) {
                        continue;
                    }
                    let next = toks.get(i + 1).map(|&u| c.text_of(u));
                    let foreach = c.keyword(n).is_some_and(|k| c.text_of(k) == "foreach");
                    if next == Some("[") && !foreach {
                        continue;
                    }
                    out.insert(x.to_string());
                }
            }
            _ => {}
        }
    }
}

fn unconstrained_rand(c: &Cst, s: &mut Sink) {
    for cls in class_decls(c) {
        let cname = decl_name(c, cls).map(|x| x.0).unwrap_or_default();
        let items = class_items(c, cls);
        let mut props: Vec<(String, u32)> = Vec::new();
        for &it in &items {
            if c.kind(it) != DataDecl || c.tree.child(it, DataType).is_none() {
                continue;
            }
            let q = qualifiers(c, it);
            if !q.contains(&"rand") || q.contains(&"static") {
                continue;
            }
            for (_, n, t) in declarators(c, it) {
                props.push((n, t));
            }
        }
        if props.is_empty() {
            continue;
        }
        let mut ids = HashSet::new();
        for &it in &items {
            if c.kind(it) == ConstraintDecl {
                identifier_names(c, it, &mut ids);
            }
        }
        for (p, at) in props {
            if ids.contains(&p) {
                continue;
            }
            s.add(
                "SV-RND-015",
                "UnconstrainedRandVariable",
                Severity::Warning,
                format!(
                    "Class property '{p}' in class '{cname}' is declared 'rand' but is never referenced in any constraint block. \
                     Unconstrained random properties generate uniformly unguided values across the entire bit-width, increasing solver cycles and frequently indicating forgotten stimulus specifications."
                ),
                c.loc(at),
            );
        }
    }
}

// ---- SV-RND-016 ----

fn randomize_target(c: &Cst, e: NodeId, out: &mut Vec<(String, u32)>) {
    match c.kind(e) {
        MemberExpr | ScopedExpr => {
            let Some(name) = c.toks(e).last() else { return };
            if c.text_of(name) != "randomize" {
                return;
            }
            let Some(left) = c.tree.children(e).next() else { return };
            let target = c.last_text(left);
            if !target.is_empty() {
                out.push((target.to_string(), name));
            }
        }
        CallExpr => {
            if let Some(callee) = c.tree.children(e).next() {
                randomize_target(c, callee, out);
            }
        }
        _ => {}
    }
}

fn concurrent_randomize(c: &Cst, s: &mut Sink) {
    for fork in c.nodes(ParBlock) {
        let mut seen: HashSet<String> = HashSet::new();
        let mut reported: HashSet<String> = HashSet::new();
        for item in c.items(fork) {
            let mut targets = Vec::new();
            for n in subtree(c, item) {
                if c.kind(n) != CallExpr {
                    continue;
                }
                // `x.randomize() with {...}` is slang's
                // ArrayOrRandomizeMethodExpression around the call.
                if let Some(callee) = c.tree.children(n).next() {
                    randomize_target(c, callee, &mut targets);
                }
            }
            for (t, at) in targets {
                if !seen.insert(t.clone()) && reported.insert(t.clone()) {
                    s.add(
                        "SV-RND-016",
                        "ConcurrentRandomizeInFork",
                        Severity::Warning,
                        format!(
                            "Concurrent call to '.randomize()' on the same object handle '{t}' across parallel fork...join threads without synchronization. Randomizing a shared object concurrently causes solver race conditions, state corruption, and non-deterministic constraint failures."
                        ),
                        c.loc(at),
                    );
                }
            }
        }
    }
}

// ---- SV-RND-020 ----

/// A dynamic array, queue or associative array dimension: `[]`, `[$]`,
/// `[$:N]`, `[*]`, `[type]`.
pub(crate) fn is_dynamic_dim(c: &Cst, d: NodeId) -> bool {
    let o = own(c, d);
    if o == ["[", "]"] && c.tree.children(d).next().is_none() {
        return true;
    }
    if o.contains(&"$") || o.contains(&"*") {
        return o.first() == Some(&"[") && o.get(1).is_some_and(|x| *x == "$" || *x == "*");
    }
    // `[string]`, `[int]`, `[my_t]`: an associative index type.
    let k = kids(c, d);
    k.len() == 1 && matches!(c.kind(k[0]), DataType | TypeExpr)
}

fn randc_dynamic(c: &Cst, s: &mut Sink) {
    for (cls, _) in model_classes(c) {
        for it in class_items(c, cls) {
            if c.kind(it) != DataDecl || !qualifiers(c, it).contains(&"randc") {
                continue;
            }
            for (d, n, t) in declarators(c, it) {
                if !c.tree.children(d).any(|x| c.kind(x) == Dim && is_dynamic_dim(c, x)) {
                    continue;
                }
                s.add(
                    "SV-RND-020",
                    "RandcInDynamicContainer",
                    Severity::Error,
                    format!(
                        "Class property '{n}' is declared as 'randc' on a dynamic container. \
                         In SystemVerilog (IEEE 1800 § 18.4.1), 'randc' cyclic variables \
                         are strictly prohibited on dynamic arrays, queues, and associative arrays."
                    ),
                    c.loc(t),
                );
            }
        }
    }
}

// ---- SV-RND-022 ----

fn non_pure_walk(c: &Cst, n: NodeId, inside: &mut bool, s: &mut Sink) {
    let k = c.kind(n);
    if matches!(k, ConstraintDecl | ConstraintBlock) {
        *inside = true;
        for x in kids(c, n) {
            non_pure_walk(c, x, inside, s);
        }
        *inside = false;
        return;
    }
    if k == CallExpr && *inside && with_clause(c, n).is_none() {
        if let Some(left) = c.tree.children(n).next() {
            let f = c.last_text(left);
            let non_pure = ["print", "display", "update", "set_", "void", "task"].iter().any(|x| f.contains(x));
            if !matches!(f, "inside" | "dist" | "size") && non_pure {
                if let Some(at) = c.first_tok(left) {
                    s.add(
                        "SV-RND-022",
                        "ConstraintCallingNonPureFunction",
                        Severity::Error,
                        format!(
                            "Function '{f}' invoked inside constraint block appears non-pure or mutates state. \
                             In SystemVerilog (IEEE 1800 § 18.5.12), functions called inside constraint \
                             expressions must be pure: they cannot modify class state, call tasks, or have void return types. \
                             Impure functions cause constraint solver instability and unpredictable solution distributions."
                        ),
                        c.loc(at),
                    );
                }
            }
        }
    }
    for x in kids(c, n) {
        non_pure_walk(c, x, inside, s);
    }
}

fn non_pure_in_constraint(c: &Cst, s: &mut Sink) {
    let mut inside = false;
    non_pure_walk(c, c.tree.root, &mut inside, s);
}

// ---- SV-RND-024 ----

fn calls_randomize(c: &Cst, e: NodeId) -> bool {
    let e = strip_casts(c, e);
    match c.kind(e) {
        CallExpr => call_name(c, e) == Some("randomize"),
        BinaryExpr if matches!(op(c, e), "&&" | "||") => kids(c, e).into_iter().take(2).any(|x| calls_randomize(c, x)),
        _ => false,
    }
}

fn randomize_in_assert(c: &Cst, s: &mut Sink) {
    {
        for st in c.nodes(AssertStmt) {
            if !matches!(op(c, st), "assert" | "assume") || !user_code(c, st) {
                continue;
            }
            let Some(cond) = c.items(st).into_iter().next() else { continue };
            if !calls_randomize(c, cond) {
                continue;
            }
            let Some(at) = c.first_tok(st) else { continue };
            s.add(
                "SV-RND-024",
                "RandomizeInsideAssert",
                Severity::Warning,
                "randomize() is called inside an assertion. When assertions are turned off \
                 ($assertoff or a simulator switch) the call is skipped and the object keeps its old \
                 values. Call it on its own: 'if (!obj.randomize()) `uvm_fatal(...)'."
                    .into(),
                c.loc(at),
            );
        }
    }
}
