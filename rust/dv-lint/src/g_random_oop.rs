//! Group E (builder slice N6.3): OOP rules (`cpp/src/rules/oop/`) and the
//! timescale and tautology checks of `cpp/src/rules/correctness/`. See
//! [`crate::g_random`].

use std::collections::{HashMap, HashSet};

use dv_model::{Design, Loc};
use sv_syntax::{Child, NodeId, Origin};

use crate::g_random::{class_decls, class_items, decl_name, declarators, kids, op, own, qualifiers, trim};
use crate::g_random_sema::{ast_ok, visits};
use crate::syntax::Cst;
use crate::Check::{Mixed, Syntax};
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-ENC-001", name: "MutableObjectGetterLeak", check: Syntax(getter_leak) },
        Rule { id: "SV-OOP-008", name: "NonVirtualInterfaceInClass", check: Syntax(non_virtual_interface) },
        Rule { id: "SV-OOP-009", name: "EmptyVirtualMethodWithoutPure", check: Syntax(empty_virtual) },
        Rule { id: "SV-OOP-011", name: "DowncastWithoutDynamicCast", check: Mixed(downcast) },
        Rule { id: "SV-OOP-012", name: "ChainedAssignmentAliasing", check: Syntax(chained_assignment) },
        Rule { id: "SV-SHD-002", name: "ForeachLoopVariableShadowing", check: Syntax(foreach_shadowing) },
        Rule { id: "SV-DUP-001", name: "DuplicateMethodBody", check: Mixed(duplicate_body) },
        Rule { id: "SV-TIM-001", name: "TimescalePrecisionMismatch", check: Syntax(timescale_precision) },
        Rule { id: "SV-TIM-002", name: "MissingTimescaleInModule", check: Syntax(missing_timescale) },
        Rule { id: "SV-COR-001", name: "TautologicalCheckCompare", check: Syntax(tautological_compare) },
    ]
}

/// A class method: function or task declared in the class body.
fn methods(c: &Cst, cls: NodeId) -> Vec<NodeId> {
    class_items(c, cls).into_iter().filter(|&x| matches!(c.kind(x), FunctionDecl | TaskDecl)).collect()
}

fn routine_name(c: &Cst, f: NodeId) -> String {
    c.tree.child(f, Name).map(|n| c.last_text(n).to_string()).unwrap_or_default()
}

/// Data declarations among the class's items.
fn data_items(c: &Cst, cls: NodeId) -> Vec<NodeId> {
    class_items(c, cls)
        .into_iter()
        .filter(|&x| c.kind(x) == DataDecl && c.tree.child(x, DataType).is_some())
        .collect()
}

// ---- SV-ENC-001 ----

const PRIMITIVE: &[&str] = &[
    "int", "integer", "bit", "logic", "reg", "byte", "shortint", "longint", "time", "real", "shortreal", "realtime",
    "string", "void", "chandle", "event",
];

fn getter_leak(c: &Cst, s: &mut Sink) {
    for cls in class_decls(c) {
        let mut objects: HashSet<String> = HashSet::new();
        for d in data_items(c, cls) {
            let ty = c.trimmed(c.tree.child(d, DataType).unwrap());
            if !PRIMITIVE.contains(&ty.as_str()) {
                for (_, n, _) in declarators(c, d) {
                    objects.insert(n);
                }
            }
        }
        for f in methods(c, cls) {
            let name = routine_name(c, f);
            let b = name.as_bytes();
            if !(name.starts_with("get_") || (name.starts_with("get") && b.len() > 3 && b[3].is_ascii_uppercase())) {
                continue;
            }
            for r in crate::g_random::subtree(c, f) {
                if c.kind(r) != ReturnStmt {
                    continue;
                }
                let Some(v) = c.tree.children(r).next() else { continue };
                if c.kind(v) != NameExpr {
                    continue;
                }
                let o = own(c, v);
                let [var] = o.as_slice() else { continue };
                if !objects.contains(*var) {
                    continue;
                }
                let Some(kw) = c.tok(r, "return") else { continue };
                s.add(
                    "SV-ENC-001",
                    "MutableObjectGetterLeak",
                    Severity::Warning,
                    format!(
                        "Getter method '{name}()' returns direct reference to mutable member object '{var}'. \
                         Returning handles to internal mutable class objects breaks OOP encapsulation and allows callers to mutate internal state without ownership control. \
                         Return a cloned copy of the object (e.g. 'return {var}.clone();') or ensure the object is immutable."
                    ),
                    c.loc(kw),
                );
            }
        }
    }
}

// ---- SV-OOP-008 ----

fn non_virtual_interface(c: &Cst, s: &mut Sink) {
    for cls in class_decls(c) {
        let cname = decl_name(c, cls).map(|x| x.0).unwrap_or_default();
        for d in data_items(c, cls) {
            let dt = c.tree.child(d, DataType).unwrap();
            let virt = qualifiers(c, d).contains(&"virtual") || op(c, dt) == "virtual";
            if virt {
                continue;
            }
            let ty = c.last_text(dt);
            if ty.starts_with("virtual") || !(ty.ends_with("_if") || ty.ends_with("_vif")) {
                continue;
            }
            for (_, var, at) in declarators(c, d) {
                s.add(
                    "SV-OOP-008",
                    "NonVirtualInterfaceInClass",
                    Severity::Error,
                    format!(
                        "Interface property '{var}' of type '{ty}' in class '{cname}' is declared without the 'virtual' keyword. In SystemVerilog (IEEE 1800 § 25.9), classes cannot instantiate physical interfaces; only virtual interface handles ('virtual {ty} {var};') can be stored as class members."
                    ),
                    c.loc(at),
                );
            }
        }
    }
}

// ---- SV-OOP-009 ----

fn empty_virtual(c: &Cst, s: &mut Sink) {
    for cls in class_decls(c) {
        let cname = decl_name(c, cls).map(|x| x.0).unwrap_or_default();
        for f in methods(c, cls) {
            let o = own(c, f);
            if !o.contains(&"virtual") || o.contains(&"pure") {
                continue;
            }
            if !o.iter().any(|t| matches!(*t, "endfunction" | "endtask")) {
                continue;
            }
            let Some(body) = c.tree.child(f, Body) else { continue };
            if c.tree.children(body).next().is_some() {
                continue;
            }
            let name = routine_name(c, f);
            if name.is_empty() || name == "new" {
                continue;
            }
            let Some(kw) = c.toks(f).find(|&t| matches!(c.text_of(t), "function" | "task")) else { continue };
            s.add(
                "SV-OOP-009",
                "EmptyVirtualMethodWithoutPure",
                Severity::Info,
                format!(
                    "Virtual method '{name}' in class '{cname}' is defined with an empty body without the 'pure' keyword. Empty stub methods allow derived classes to silently omit required overrides. Declare as 'pure virtual' in interface base classes or provide meaningful default logic."
                ),
                c.loc(kw),
            );
        }
    }
}

// ---- SV-OOP-011 ----

fn downcast(d: &Design, csts: &[Cst], s: &mut Sink) {
    let mut known: HashSet<String> = d.classes().iter().filter(|k| !k.is_parameterized).map(|k| k.name.clone()).collect();
    const HINTS: &[&str] =
        &["uvm_", "_item", "_seq", "_driver", "_monitor", "_scoreboard", "_env", "_component", "_object"];
    for c in csts {
        for n in c.tree.preorder() {
            match c.kind(n) {
                ClassDecl => {
                    if let Some((name, _)) = decl_name(c, n) {
                        if !name.is_empty() {
                            known.insert(name);
                        }
                    }
                }
                CastExpr if own(c, n).contains(&"(") => {
                    let Some(left) = c.tree.children(n).next() else { continue };
                    // `void'(f());` as a statement is slang's
                    // VoidCastedCallStatement, not a cast expression.
                    if c.tree.bare_text(left) == "void" && c.parent(n).is_some_and(|p| c.kind(p) == ExprStmt) {
                        continue;
                    }
                    let ty = c.trimmed(left);
                    if !known.contains(&ty) && !HINTS.iter().any(|h| ty.contains(h)) {
                        continue;
                    }
                    let Some(at) = c.first_tok(n) else { continue };
                    s.add(
                        "SV-OOP-011",
                        "DowncastWithoutDynamicCast",
                        Severity::Error,
                        format!(
                            "Explicit static cast '{ty}'(...)' used on class handle. \
                             In SystemVerilog (IEEE 1800 § 8.16), downcasting class handles requires dynamic casting via \
                             '$cast(target, source)' to verify polymorphic type compatibility at runtime. \
                             Static class casting bypasses runtime type safety and causes simulator aborts. Replace with '$cast()'."
                        ),
                        c.loc(at),
                    );
                }
                _ => {}
            }
        }
    }
}

// ---- SV-OOP-012 ----

fn chained_assignment(c: &Cst, s: &mut Sink) {
    let is_assign = |n: NodeId| c.kind(n) == BinaryExpr && op(c, n) == "=";
    for n in c.nodes(BinaryExpr) {
        if !is_assign(n) {
            continue;
        }
        let k = kids(c, n);
        if !k.iter().take(2).any(|&x| is_assign(x)) {
            continue;
        }
        let Some(at) = c.toks(n).next() else { continue };
        s.add(
            "SV-OOP-012",
            "ChainedAssignmentAliasing",
            Severity::Warning,
            "Chained assignment expression detected. \
             In SystemVerilog verification environments, chained assignments (e.g. 'a = b = c;') create shared object aliases \
             across multiple references without clone safety, leading to unintentional state mutations across threads. \
             Split chained assignments into discrete statements."
                .into(),
            c.loc(at),
        );
    }
}

// ---- SV-SHD-002 ----

/// The loop variables of a `foreach (a[i, j])` header: names and tokens.
fn foreach_vars(c: &Cst, l: NodeId) -> Vec<(String, u32)> {
    let toks: Vec<u32> = c.toks(l).collect();
    // The last top-level `[` before the closing `)`.
    let close = toks.iter().rposition(|&t| c.text_of(t) == ")");
    let Some(close) = close else { return Vec::new() };
    let Some(open) = toks[..close].iter().rposition(|&t| c.text_of(t) == "[") else { return Vec::new() };
    let mut out = Vec::new();
    let mut cur: Option<u32> = None;
    for &t in &toks[open + 1..close] {
        match c.text_of(t) {
            "," | "]" => {
                if let Some(v) = cur.take() {
                    out.push((c.text_of(v).to_string(), v));
                }
            }
            _ => cur = Some(t),
        }
    }
    out
}

fn shadow_walk(c: &Cst, n: NodeId, props: &mut HashSet<String>, locals: &mut HashSet<String>, s: &mut Sink) {
    match c.kind(n) {
        ClassDecl => {
            let prev = std::mem::take(props);
            for d in class_items(c, n).into_iter().filter(|&x| c.kind(x) == DataDecl) {
                for (_, v, _) in declarators(c, d) {
                    props.insert(v);
                }
            }
            for x in kids(c, n) {
                shadow_walk(c, x, props, locals, s);
            }
            *props = prev;
            return;
        }
        FunctionDecl | TaskDecl => {
            let prev = std::mem::take(locals);
            if let Some(pl) = c.tree.child(n, PortList) {
                for p in c.tree.children(pl).filter(|&x| c.kind(x) == Port) {
                    for (_, v, _) in declarators(c, p) {
                        locals.insert(v);
                    }
                }
            }
            if let Some(body) = c.tree.child(n, Body) {
                for d in c.tree.children(body).filter(|&x| c.kind(x) == DataDecl) {
                    for (_, v, _) in declarators(c, d) {
                        locals.insert(v);
                    }
                }
            }
            for x in kids(c, n) {
                shadow_walk(c, x, props, locals, s);
            }
            *locals = prev;
            return;
        }
        LoopStmt if c.keyword(n).is_some_and(|k| c.text_of(k) == "foreach") => {
            for (v, at) in foreach_vars(c, n) {
                let msg = if props.contains(&v) {
                    format!(
                        "Foreach loop index variable '{v}' shadows class property '{v}'. Per IEEE 1800 § 12.7.3, foreach loop variables are implicitly declared in the local loop scope and shadow outer identifiers with the same name, masking class state. Rename the loop index (e.g. '{v}_idx')."
                    )
                } else if locals.contains(&v) {
                    format!(
                        "Foreach loop index variable '{v}' shadows local variable '{v}' declared in the enclosing routine. Per IEEE 1800 § 12.7.3, foreach loop variables create a nested scope, masking routine variables. Rename the loop index (e.g. '{v}_idx')."
                    )
                } else {
                    continue;
                };
                s.add("SV-SHD-002", "ForeachLoopVariableShadowing", Severity::Warning, msg, c.loc(at));
            }
        }
        _ => {}
    }
    for x in kids(c, n) {
        shadow_walk(c, x, props, locals, s);
    }
}

fn foreach_shadowing(c: &Cst, s: &mut Sink) {
    let (mut props, mut locals) = (HashSet::new(), HashSet::new());
    shadow_walk(c, c.tree.root, &mut props, &mut locals, s);
}

// ---- SV-DUP-001 ----

/// A node's text exactly as its file holds it, from its first token to the
/// end of its last; `None` when either end comes from a macro.
fn file_span(c: &Cst, n: NodeId) -> Option<(u32, String)> {
    let (_, b) = c.tree.token_bounds(n)?;
    // slang's FunctionDeclarationSyntax starts at `function` / `task`,
    // after the class method's qualifiers.
    let a = c.toks(n).find(|&t| matches!(c.text_of(t), "function" | "task"))?;
    let (ta, tb) = (&c.tree.tokens[a as usize], &c.tree.tokens[b as usize]);
    if ta.from_macro || tb.from_macro || ta.origin.file != tb.origin.file {
        return None;
    }
    let src = &c.tree.sources[ta.origin.file as usize];
    let (s0, e0) = (ta.origin.offset as usize, tb.origin.offset as usize + tb.text.len());
    (e0 > s0 && e0 <= src.len()).then(|| (ta.origin.file, src[s0..e0].to_string()))
}

fn normalize_body(raw: &str) -> String {
    let b = raw.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut line = false;
    let mut i = 0;
    while i < b.len() {
        let ch = b[i];
        if line {
            if ch == b'\n' {
                line = false;
            }
        } else if ch == b'/' && b.get(i + 1) == Some(&b'/') {
            line = true;
        } else if !matches!(ch, b' ' | b'\t' | b'\r' | b'\n') {
            out.push(ch);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn duplicate_body(_: &Design, csts: &[Cst], s: &mut Sink) {
    let mut known: HashMap<String, (String, String, Loc)> = HashMap::new();
    for c in csts {
        for cls in crate::g_random::class_decls(c) {
            if crate::g_random::is_parameterized(c, cls) {
                continue;
            }
            let Some((cname, _)) = decl_name(c, cls) else { continue };
            for f in methods(c, cls) {
                let name = routine_name(c, f);
                if name == "new" || name.contains("::") {
                    continue;
                }
                let Some(name_tok) = c.tree.child(f, Name).and_then(|n| c.tree.token_bounds(n)).map(|x| x.1) else {
                    continue;
                };
                let Some((_, raw)) = file_span(c, f) else { continue };
                let norm = normalize_body(&raw);
                if norm.len() < 50 {
                    continue;
                }
                match known.get(&norm) {
                    Some((kc, km, kl)) => {
                        if *kc != cname {
                            let f = s.add(
                                "SV-DUP-001",
                                "DuplicateMethodBody",
                                Severity::Warning,
                                format!(
                                    "Subroutine '{name}' in class '{cname}' has an identical body to subroutine '{km}' in class '{kc}'. Refactor duplicate code into a common base class or shared package utility."
                                ),
                                c.loc(name_tok),
                            );
                            f.related.push((*kl, format!("first copy, {kc}::{km}")));
                        }
                    }
                    None => {
                        known.insert(norm, (cname.clone(), name.clone(), c.loc(name_tok)));
                    }
                }
            }
        }
    }
}

// ---- SV-TIM-001 / SV-TIM-002 ----

fn time_exp(s: &str) -> i32 {
    let mut s = s.trim_start_matches([' ', '\t']).trim_end_matches([' ', '\t', '\r', '\n', ';']);
    let mult = if let Some(r) = s.strip_prefix("100") {
        s = r;
        2
    } else if let Some(r) = s.strip_prefix("10") {
        s = r;
        1
    } else if let Some(r) = s.strip_prefix('1') {
        s = r;
        0
    } else {
        return 999;
    };
    let base = match s.trim_start_matches([' ', '\t']) {
        "s" => 0,
        "ms" => -3,
        "us" => -6,
        "ns" => -9,
        "ps" => -12,
        "fs" => -15,
        _ => return 999,
    };
    base + mult
}

/// The files given to the compiler (not included ones): the tree's first.
fn main_files(c: &Cst) -> Vec<u32> {
    c.tree.mains.clone()
}

fn timescale_precision(c: &Cst, s: &mut Sink) {
    for f in main_files(c) {
        let src = &c.tree.sources[f as usize];
        let b = src.as_bytes();
        let mut pos = 0;
        while let Some(i) = src[pos..].find("`timescale") {
            let at = pos + i;
            if at == 0 || matches!(b[at - 1], b'\n' | b' ' | b'\t') {
                let end = src[at..].find(['\r', '\n']).map_or(src.len(), |e| at + e);
                let line = &src[at + 10..end.max(at + 10)];
                if let Some(slash) = line.find('/') {
                    let (unit, prec) = (&line[..slash], &line[slash + 1..]);
                    let (ue, pe) = (time_exp(unit), time_exp(prec));
                    if ue != 999 && pe != 999 && pe > ue {
                        let cu = unit.trim_matches([' ', '\t']);
                        let cp = prec.trim_end_matches([' ', '\t', '\r', '\n', ';']).trim_start_matches([' ', '\t']);
                        s.add(
                            "SV-TIM-001",
                            "TimescalePrecisionMismatch",
                            Severity::Error,
                            format!(
                                "Timescale directive specifies a time precision ('{cp}') coarser than the time unit ('{cu}'). Per IEEE 1800 § 3.14.2, time precision must be smaller than or equal to the time unit to prevent simulation rounding errors."
                            ),
                            c.loc_at(Origin { file: f, offset: at as u32 }),
                        );
                    }
                }
            }
            pos = at + 10;
        }
    }
}

fn has_timescale(src: &str) -> bool {
    for (word, len) in [("`timescale", 10), ("timeunit", 8)] {
        let mut pos = 0;
        while let Some(i) = src[pos..].find(word) {
            let at = pos + i;
            let start = src[..at].rfind('\n').map_or(0, |x| x + 1);
            let prefix = &src[start..at];
            if !prefix.contains("//") && !prefix.contains("/*") {
                return true;
            }
            pos = at + len;
        }
    }
    false
}

fn missing_timescale(c: &Cst, s: &mut Sink) {
    for f in main_files(c) {
        if has_timescale(&c.tree.sources[f as usize]) {
            continue;
        }
        // slang's DelaySyntax (`#d`, `##n`) and TimingControlStatementSyntax.
        // Sequence and property delays are not DelaySyntax.
        let mut first = None;
        let mut stack = vec![c.tree.root];
        while let Some(n) = stack.pop() {
            match c.kind(n) {
                SequenceDecl | PropertyDecl | SequenceExpr | PropertyExpr | ConcurrentAssert => continue,
                TimedStmt => {
                    first = Some(n);
                    break;
                }
                // `#1step` (a clocking skew) and a gate's `#(1, 2)` (a
                // parameter assignment to slang) are not delays.
                TimingControl
                    if matches!(op(c, n), "#" | "##")
                        && c.trimmed(n) != "#1step"
                        && !(c.parent(n).is_some_and(|p| matches!(c.kind(p), GateInst | Instantiation))
                            && own(c, n).get(1) == Some(&"(")) =>
                {
                    first = Some(n);
                    break;
                }
                _ => {}
            }
            let k: Vec<NodeId> = c.tree.children(n).collect();
            stack.extend(k.into_iter().rev());
        }
        let Some(at) = first.and_then(|n| c.first_tok(n)) else { continue };
        s.add(
            "SV-TIM-002",
            "MissingTimescaleInModule",
            Severity::Warning,
            "File contains procedural timing controls ('#delay') but omits a '`timescale' or 'timeunit' directive. Per IEEE 1800 § 3.14, omitting explicit timescale specifications causes simulators to adopt implementation-dependent default time units, introducing unpredictable timing skew and race conditions across VIP components."
                .into(),
            c.loc(at),
        );
    }
}

// ---- SV-COR-001 ----

/// slang's IntegerLiteral or RealLiteral (not strings, times or `'0`).
pub(crate) fn is_number_literal(c: &Cst, n: NodeId) -> bool {
    if c.kind(n) != Literal {
        return false;
    }
    let t = c.trimmed(n);
    let t = t.as_str();
    let Some(f) = t.chars().next() else { return false };
    if f == '"' {
        return false;
    }
    if f == '\'' {
        // `'h1F` is an integer; `'0`, `'1`, `'x`, `'z` are unbased unsized.
        return t.len() > 2 || t.chars().nth(1).is_some_and(|x| matches!(x, 'h' | 'H' | 'd' | 'D' | 'b' | 'B' | 'o' | 'O' | 's' | 'S'));
    }
    if !f.is_ascii_digit() {
        return false;
    }
    // A time literal ends in its unit.
    !["fs", "ps", "ns", "us", "ms", "s", "step"].iter().any(|u| t.ends_with(u) && !t.contains('\''))
}

fn tautological_compare(c: &Cst, s: &mut Sink) {
    for n in c.nodes(BinaryExpr) {
        if !matches!(op(c, n), "==" | "!=" | "===" | "!==" | "==?" | "!=?" | "<" | "<=" | ">" | ">=") {
            continue;
        }
        // `<=` as a statement's operator is a nonblocking assignment.
        if op(c, n) == "<=" && c.parent(n).is_some_and(|p| matches!(c.kind(p), ExprStmt)) {
            continue;
        }
        let k = kids(c, n);
        let (Some(&l), Some(&r)) = (k.first(), k.get(1)) else { continue };
        let ((l, ls), (r, rs)) = (strip_conv(c, l), strip_conv(c, r));
        if is_number_literal(c, unparen(c, l)) || is_number_literal(c, unparen(c, r)) {
            continue;
        }
        let (ls, rs) = (trim(&ls).to_string(), trim(&rs).to_string());
        if ls.is_empty() || ls != rs || !ls.chars().any(|x| x.is_ascii_alphabetic() || x == '_') {
            continue;
        }
        if !ast_ok(c, n) {
            continue;
        }
        let Some(at) = c.first_tok(n) else { continue };
        for _ in 0..visits(c, n) {
            s.add(
                "SV-COR-001",
                "TautologicalCheckCompare",
                Severity::Error,
                format!(
                    "Tautological comparison between identical expressions '{ls}' and '{rs}'. \
                     In verification checks and scoreboards, comparing an expression to itself always evaluates to a constant \
                     result, masking functional RTL bugs and creating silent false-positive test results."
                ),
                c.loc(at),
            );
        }
    }
}

/// Explicit casts are conversions in slang's AST; the operand keeps the
/// cast's parentheses in its syntax (`int'(j)` gives `(j)`). Gives the
/// operand and that text.
pub(crate) fn strip_conv(c: &Cst, e: NodeId) -> (NodeId, String) {
    let mut e = e;
    let mut text = c.text(e);
    while c.kind(e) == CastExpr {
        let (Some(open), Some(close)) = (c.tok(e, "("), c.toks(e).filter(|&t| c.text_of(t) == ")").last()) else {
            break;
        };
        let Some(x) = c.tree.children(e).nth(1) else { break };
        text = crate::g_random::span_text(c, open, close);
        e = x;
    }
    (e, text)
}

/// slang drops parentheses from the bound expression.
pub(crate) fn unparen(c: &Cst, mut e: NodeId) -> NodeId {
    while c.kind(e) == ParenExpr {
        match c.tree.children(e).next() {
            Some(x) => e = x,
            None => break,
        }
    }
    e
}

#[allow(dead_code)]
fn tokens_of(c: &Cst, n: NodeId) -> Vec<u32> {
    c.kids(n).iter().filter_map(|k| if let Child::Token(t) = *k { Some(t) } else { None }).collect()
}
