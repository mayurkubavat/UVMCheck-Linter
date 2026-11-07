//! Rules on expression types (builder slice N6.4, part 4), on top of the
//! expression typer (g_exprty), which gives each expression the type slang's
//! `Expression::type` holds. Messages are verbatim.

use sv_syntax::NodeId;
use sv_syntax::SyntaxKind::*;

use crate::g_c_support::kids;
use crate::g_exprty::{expr_types, ETy};
use crate::g_port3::ast_visits;
use crate::syntax::Cst;
use crate::Check::Syntax;
use crate::{Rule, Severity, Sink};

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-TYP-001", name: "MixedSignComparison", check: Syntax(mixed_sign_comparison) },
        Rule { id: "SV-TYP-008", name: "RedundantCastToSameType", check: Syntax(redundant_cast) },
        Rule { id: "SV-RDB-005", name: "RedundantBoolean", check: Syntax(redundant_boolean) },
        Rule { id: "SV-RDB-001", name: "PositionalFlagLiterals", check: Syntax(positional_flags) },
        Rule { id: "SV-PRF-002", name: "ArrayReductionWidthTruncation", check: Syntax(array_reduction_width) },
        Rule { id: "SV-RDB-006", name: "DoubleNegativeFlag", check: Syntax(double_negative_flag) },
    ]
}

/// In a generate construct's header (a loop's bounds, an `if` or `case`
/// condition): elaboration evaluates it, the AST walk does not reach it.
pub(crate) fn in_generate_header(c: &Cst, e: NodeId) -> bool {
    for a in crate::g_c_support::ancestors(c, e) {
        match c.kind(a) {
            LoopGenerate | IfGenerate | CaseGenerate => return true,
            GenerateBlock | ElseClause | ModuleDecl | InterfaceDecl | ProgramDecl | ClassDecl | FunctionDecl | TaskDecl
            | ProceduralBlock | ContinuousAssign | DataDecl | Instantiation => return false,
            _ => {}
        }
    }
    false
}

/// The operand under explicit casts and parentheses, as a rule sees it
/// after stripping conversions.
fn strip(c: &Cst, mut e: NodeId) -> NodeId {
    loop {
        match c.kind(e) {
            ParenExpr => match kids(c, e).first() {
                Some(&i) => e = i,
                None => return e,
            },
            CastExpr => match kids(c, e).get(1) {
                Some(&i) => e = i,
                None => return e,
            },
            _ => return e,
        }
    }
}

/// An integer literal (not `'0`) whose value is not negative.
fn non_negative_literal(c: &Cst, e: NodeId, t: &ETy) -> bool {
    if crate::g_exprty::slang_kind(c, e) != Some("IntegerLiteral") {
        return false;
    }
    let text = c.tree.bare_text(e).replace('_', "");
    // A signed based literal with its top bit set is negative.
    let Some(q) = text.find('\'') else { return true };
    let rest = &text[q + 1..];
    if !rest.starts_with(['s', 'S']) {
        return true;
    }
    let (w, ..) = t.integral.unwrap_or((32, true, false));
    let digits = &rest[2..];
    let bits: String = match rest.as_bytes().get(1).map(|b| b.to_ascii_lowercase()) {
        Some(b'b') => digits.to_string(),
        Some(b'h') => digits.chars().map(|d| d.to_digit(16).map_or("xxxx".into(), |v| format!("{v:04b}"))).collect(),
        Some(b'o') => digits.chars().map(|d| d.to_digit(8).map_or("xxx".into(), |v| format!("{v:03b}"))).collect(),
        Some(b'd') => return digits.parse::<u128>().map_or(true, |v| w >= 128 || v >> (w - 1) == 0),
        _ => return true,
    };
    let bits = bits.trim_start_matches('0');
    (bits.len() as u32) < w || !bits.starts_with('1')
}

const RELATIONAL: &[&str] = &["<", "<=", ">", ">=", "==", "!=", "===", "!=="];

/// A comparison of a signed and an unsigned operand, unless the signed one
/// is a non-negative literal: once per AST visit.
fn mixed_sign_comparison(c: &Cst, out: &mut Sink) {
    let types = expr_types(c);
    let ix = c.index();
    for e in c.nodes(BinaryExpr) {
        let op = c.keyword(e).map_or("", |k| c.text_of(k));
        if !RELATIONAL.contains(&op) || (op == "<=" && c.parent(e).is_some_and(|p| c.kind(p) == ExprStmt)) {
            continue;
        }
        let ks = kids(c, e);
        let (Some(&l), Some(&r)) = (ks.first(), ks.get(1)) else { continue };
        let (l, r) = (strip(c, l), strip(c, r));
        let (Some(lt), Some(rt)) = (types.get(&(ix, l)), types.get(&(ix, r))) else { continue };
        let (Some((_, ls, _)), Some((_, rs, _))) = (lt.integral, rt.integral) else { continue };
        if ls == rs {
            continue;
        }
        let flag = (ls && !non_negative_literal(c, l, lt)) || (rs && !non_negative_literal(c, r, rt));
        if !flag {
            continue;
        }
        if in_generate_header(c, e) {
            continue;
        }
        let side = if ls { "LHS" } else { "RHS" };
        let Some(t) = c.first_tok(e) else { continue };
        for _ in 0..ast_visits(c, e) {
            out.add(
                "SV-TYP-001",
                "MixedSignComparison",
                Severity::Warning,
                format!(
                    "Comparison between signed type '{}' and unsigned type '{}'. In SystemVerilog (IEEE 1800 Sec 11.8.2), if either operand is unsigned, the entire comparison is coerced to unsigned, causing negative numbers on the {side} to evaluate as large positive numbers.",
                    lt.text, rt.text
                ),
                c.loc(t),
            );
        }
    }
}

/// `$cast(a, b)` on matching types, or a cast to the operand's own type:
/// once per AST visit.
fn redundant_cast(c: &Cst, out: &mut Sink) {
    let types = expr_types(c);
    let canon = crate::g_exprty::cast_canon(c);
    let specs = crate::g_exprty::spec_types(c);
    let ix = c.index();
    let matching = |a: (NodeId, &ETy), b: (NodeId, &ETy)| {
        a.1.text == b.1.text || canon.get(&(ix, a.0)).is_some_and(|x| canon.get(&(ix, b.0)) == Some(x))
    };
    // The type `a` has in each copy where `a` and `b` match: per
    // specialization in a generic class, else every AST visit.
    let matches = |e: NodeId, a: NodeId, b: NodeId| -> Vec<String> {
        if let (Some(x), Some(y)) = (specs.get(&(ix, a)), specs.get(&(ix, b))) {
            return x
                .iter()
                .zip(y)
                .filter_map(|p| match p {
                    (Some((at, ac)), Some((bt, bc))) if at.text == bt.text || ac == bc => Some(at.text.clone()),
                    _ => None,
                })
                .collect();
        }
        let (Some(at), Some(bt)) = (types.get(&(ix, a)), types.get(&(ix, b))) else { return Vec::new() };
        if !matching((a, at), (b, bt)) {
            return Vec::new();
        }
        vec![at.text.clone(); ast_visits(c, e)]
    };
    for e in c.nodes(CallExpr) {
        let ks = kids(c, e);
        let Some(&callee) = ks.first() else { continue };
        if c.kind(callee) != NameExpr || c.tree.bare_text(callee) != "$cast" {
            continue;
        }
        let args = crate::g_exprty::call_args(c, e);
        let (Some(&d), Some(&s)) = (args.first(), args.get(1)) else { continue };
        if in_generate_header(c, e) {
            continue;
        }
        let Some(t) = c.first_tok(e) else { continue };
        for text in matches(e, d, s) {
            out.add(
                "SV-TYP-008",
                "RedundantCastToSameType",
                Severity::Warning,
                format!(
                    "Redundant dynamic cast '$cast(...)': destination and source both share identical static type '{}'. In SystemVerilog typing rules (IEEE 1800 \u{a7} 6.24), dynamic casting between identical types is unnecessary; use standard assignment '=' instead.",
                    text
                ),
                c.loc(t),
            );
        }
    }
    for e in c.nodes(CastExpr) {
        let ks = kids(c, e);
        let Some(&inner) = ks.get(1) else { continue };
        let inner = strip_parens(c, inner);
        if in_generate_header(c, e) {
            continue;
        }
        let Some(t) = c.first_tok(e) else { continue };
        for text in matches(e, e, inner) {
            out.add(
                "SV-TYP-008",
                "RedundantCastToSameType",
                Severity::Info,
                format!(
                    "Redundant static cast to identical type '{}'. In SystemVerilog typing rules (IEEE 1800 \u{a7} 6.24), casting an expression to its already established static type adds unnecessary verbosity.",
                    text
                ),
                c.loc(t),
            );
        }
    }
}

fn strip_parens(c: &Cst, mut e: NodeId) -> NodeId {
    while c.kind(e) == ParenExpr {
        match kids(c, e).first() {
            Some(&i) => e = i,
            None => break,
        }
    }
    e
}

// ---- SV-RDB-005 ----

/// How often `swe::UserCodeVisitor` reaches `n`: never in library code,
/// each elaborated copy of module code, once in a class (its default
/// specialization).
fn user_visits(c: &Cst, n: NodeId) -> usize {
    if crate::g_c_support::is_library(c, n) {
        return 0;
    }
    if crate::g_c_support::ancestors(c, n).any(|a| matches!(c.kind(a), ModuleDecl | InterfaceDecl | ProgramDecl | CheckerDecl)) {
        if let Some(m) = crate::g_c_support::elab_copies(c, n) {
            return m;
        }
    }
    crate::g_sva_cov::user_code_times(c, n)
}

/// An expression with the parentheses around it: slang binds `(x)` to `x`
/// with the parenthesized syntax.
fn with_parens(c: &Cst, mut e: NodeId) -> NodeId {
    while let Some(p) = c.parent(e).filter(|&p| c.kind(p) == ParenExpr) {
        e = p;
    }
    e
}

/// 0 or 1 for a 0/1 integer literal (under casts), else None.
fn bool_literal(c: &Cst, e: NodeId) -> Option<i128> {
    let x = strip(c, e);
    if crate::g_exprty::slang_kind(c, x) != Some("IntegerLiteral") {
        return None;
    }
    let text = c.tree.bare_text(x).replace('_', "");
    let digits = match text.find('\'') {
        Some(q) => {
            let rest = text[q + 1..].trim_start_matches(['s', 'S']);
            let radix = match rest.as_bytes().first().map(|b| b.to_ascii_lowercase()) {
                Some(b'b') => 2,
                Some(b'o') => 8,
                Some(b'd') => 10,
                Some(b'h') => 16,
                _ => return None,
            };
            i128::from_str_radix(&rest[1..], radix).ok()?
        }
        None => text.parse::<i128>().ok()?,
    };
    matches!(digits, 0 | 1).then_some(digits)
}

/// A statement under `begin ... end` blocks of one statement.
fn unwrap_stmt(c: &Cst, mut s: NodeId) -> NodeId {
    while c.kind(s) == SeqBlock {
        let items = c.items(s);
        let stmts: Vec<NodeId> = items.iter().copied().filter(|&x| crate::g_design::stmt(c, x).is_some()).collect();
        match stmts.as_slice() {
            [one] if crate::g_design::stmt(c, *one) != Some(crate::g_design::St::Decl) => s = *one,
            _ => break,
        }
    }
    s
}

fn returned_bool(c: &Cst, s: NodeId) -> Option<i128> {
    let x = unwrap_stmt(c, s);
    if c.kind(x) != ReturnStmt {
        return None;
    }
    let e = kids(c, x).into_iter().find(|&k| crate::g_exprty::is_expr(c.kind(k)))?;
    bool_literal(c, e)
}

/// `b == 1'b1` on a 1-bit `b`, and `if (c) return 1; else return 0;`.
fn redundant_boolean(c: &Cst, out: &mut Sink) {
    let types = expr_types(c);
    let ix = c.index();
    let text = crate::g_random_bug::expr_text;
    let mut found: Vec<(NodeId, String)> = Vec::new();
    for e in c.nodes(BinaryExpr) {
        let op = c.keyword(e).map_or("", |k| c.text_of(k));
        if !matches!(op, "==" | "!=") {
            continue;
        }
        let ks = kids(c, e);
        let (Some(&l), Some(&r)) = (ks.first(), ks.get(1)) else { continue };
        let (lb, rb) = (bool_literal(c, l), bool_literal(c, r));
        let (other, lit) = match (lb, rb) {
            (Some(v), _) => (r, v),
            (None, Some(v)) => (l, v),
            _ => continue,
        };
        if bool_literal(c, other).is_some() {
            continue;
        }
        let o = strip(c, other);
        if !types.get(&(ix, o)).and_then(|t| t.integral).is_some_and(|(w, ..)| w == 1) {
            continue;
        }
        if in_generate_header(c, e) {
            continue;
        }
        let bang = if (lit == 1) == (op == "==") { "" } else { "!" };
        found.push((
            e,
            format!(
                "'{}' compares a 1-bit value with a constant; write '{bang}{}' instead.",
                text(c, with_parens(c, e)),
                text(c, with_parens(c, o))
            ),
        ));
    }
    for n in c.nodes(IfStmt) {
        let (cond, then, els) = crate::g_design::if_parts(c, n);
        let (Some(cond), Some(then), Some(els)) = (cond, then, els) else { continue };
        let chained = c.parent(n).filter(|&p| c.kind(p) == ElseClause).is_some();
        if chained || c.kind(cond) == MatchExpr || c.toks(cond).any(|t| c.text_of(t) == "&&&") {
            continue;
        }
        let (Some(t), Some(f)) = (returned_bool(c, then), returned_bool(c, els)) else { continue };
        if t == f {
            continue;
        }
        found.push((
            n,
            format!(
                "'if (c) return {t}; else return {f};' can be written 'return {}({});'.",
                if t == 1 { "" } else { "!" },
                text(c, cond)
            ),
        ));
    }
    for (n, msg) in found {
        let times = user_visits(c, n);
        if times == 0 || crate::g_design::in_bad_body(c, n) {
            continue;
        }
        let Some(t) = c.first_tok(n) else { continue };
        for _ in 0..times {
            out.add("SV-RDB-005", "RedundantBoolean", Severity::Info, msg.clone(), c.loc(t));
        }
    }
}

// ---- SV-RDB-001 ----

/// The `min_flags` default.
const MIN_FLAGS: usize = 2;

/// A call of a user routine passing two or more 0/1 literals positionally
/// to one-bit formals.
fn positional_flags(c: &Cst, out: &mut Sink) {
    let one_bit = crate::g_exprty::one_bit_formals(c);
    let ix = c.index();
    for call in c.nodes(CallExpr) {
        let Some(&callee) = kids(c, call).first() else { continue };
        if c.first_tok(callee).is_some_and(|t| c.text_of(t).starts_with('$')) {
            continue;
        }
        let Some(decl) = crate::g_c_support::callee_decl(c, call) else { continue };
        if crate::g_c_support::is_library(c, decl) {
            continue;
        }
        let times = user_visits(c, call);
        if times == 0 {
            continue;
        }
        // Formals in order.
        let formals: Vec<NodeId> = c
            .tree
            .child(decl, PortList)
            .map(|pl| {
                c.tree
                    .children(pl)
                    .filter(|&p| c.kind(p) == Port)
                    .flat_map(|p| c.tree.children(p).filter(|&d| c.kind(d) == Declarator).collect::<Vec<_>>())
                    .collect()
            })
            .unwrap_or_default();
        // Leading positional arguments.
        let Some(al) = c.tree.child(call, ArgList) else { continue };
        let mut names = Vec::new();
        for (i, a) in c.tree.children(al).filter(|&a| c.kind(a) == Arg).enumerate() {
            if c.tok(a, ".").is_some() {
                break;
            }
            let (Some(&f), Some(&e)) = (formals.get(i), kids(c, a).last()) else { continue };
            if one_bit.contains(&(ix, f)) && bool_literal(c, e).is_some() {
                names.push(c.tree.child(f, Name).map(|n| c.tree.bare_text(n)).unwrap_or_default());
            }
        }
        if names.len() < MIN_FLAGS {
            continue;
        }
        let named: Vec<String> = names.iter().map(|n| format!(".{n}(...)")).collect();
        let name = crate::g_c_support::last_name(c, decl);
        let Some(t) = c.first_tok(call) else { continue };
        for _ in 0..times {
            out.add(
                "SV-RDB-001",
                "PositionalFlagLiterals",
                Severity::Info,
                format!(
                    "Call to '{name}' passes {} positional 0/1 flags. A reader cannot tell which is which, and adding an argument shifts them silently; bind them by name: {}.",
                    names.len(),
                    named.join(", ")
                ),
                c.loc(t),
            );
        }
    }
}

// ---- SV-PRF-002 ----

/// `x = a.sum()` (or `.product()`) into a target wider than the elements:
/// once per AST visit.
fn array_reduction_width(c: &Cst, out: &mut Sink) {
    let types = expr_types(c);
    let ix = c.index();
    for e in c.nodes(BinaryExpr) {
        let op = c.keyword(e).map_or("", |k| c.text_of(k));
        let assign = matches!(op, "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "&=" | "|=" | "^=" | "<<=" | ">>=" | "<<<=" | ">>>=")
            || (op == "<=" && c.parent(e).is_some_and(|p| c.kind(p) == ExprStmt));
        if !assign {
            continue;
        }
        let ks = kids(c, e);
        let (Some(&l), Some(&r)) = (ks.first(), ks.get(1)) else { continue };
        let call = strip(c, r);
        if c.kind(call) != CallExpr {
            continue;
        }
        let Some(&callee) = kids(c, call).first() else { continue };
        let name = c.last_text(callee);
        if c.kind(callee) != MemberExpr || !matches!(name, "sum" | "product") {
            continue;
        }
        let lw = types.get(&(ix, l)).and_then(|t| t.integral).map_or(0, |x| x.0);
        let Some(ew) = types.get(&(ix, call)).and_then(|t| t.integral).map(|x| x.0) else { continue };
        // Any `with` in the call's text.
        if lw <= ew || c.tree.bare_text(call).contains("with") {
            continue;
        }
        let Some(t) = c.first_tok(call) else { continue };
        for _ in 0..ast_visits(c, e) {
            out.add(
                "SV-PRF-002",
                "ArrayReductionWidthTruncation",
                Severity::Error,
                format!(
                    "Array reduction method '.{name}()' accumulates using the array element width ({ew} bits) before assignment to wider target ({lw} bits). Sum/product will silently wrap and truncate. Use with clause to expand accumulator, e.g. '.sum(item) with (int'(item))'."
                ),
                c.loc(t),
            );
        }
    }
}

// ---- SV-RDB-006 ----

/// The words of a name: split at `_`, digits and lower-to-upper case steps,
/// lowercased.
fn ident_words(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for ch in name.chars() {
        if ch == '_' || ch.is_ascii_digit() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur).to_lowercase());
            }
            continue;
        }
        if ch.is_ascii_uppercase() && cur.chars().last().is_some_and(|l| l.is_ascii_lowercase()) {
            out.push(std::mem::take(&mut cur).to_lowercase());
        }
        cur.push(ch);
    }
    if !cur.is_empty() {
        out.push(cur.to_lowercase());
    }
    out
}

fn negative_name(name: &str) -> bool {
    const NEG: &[&str] = &["disable", "disabled", "dis", "no", "not", "inhibit", "skip", "ignore", "without", "non", "suppress"];
    ident_words(name).iter().any(|w| NEG.contains(&w.as_str()))
}

/// A variable (not a net, argument or parameter) or a class property.
fn user_variable(c: &Cst, d: &crate::g_c_support::Decl) -> bool {
    use crate::g_c_support::DeclKind::*;
    match d.kind {
        Property | Local => true,
        Static => {
            const NETS: &[&str] = &["wire", "tri", "wand", "wor", "triand", "trior", "tri0", "tri1", "supply0", "supply1", "uwire", "trireg"];
            // The declaration holding the declarator.
            let Some(holder) = c.parent(d.node) else { return false };
            if c.toks(holder).any(|t| NETS.contains(&c.text_of(t))) || d.dtype.is_some_and(|t| c.toks(t).any(|t| NETS.contains(&c.text_of(t)))) {
                return false;
            }
            // A port without a data type is a net.
            if c.kind(holder) == Port {
                // An input or inout port is a net unless declared `var`.
                let toks: Vec<&str> = c.toks(holder).map(|t| c.text_of(t)).collect();
                if toks.iter().any(|t| matches!(*t, "input" | "inout")) && !toks.contains(&"var") {
                    return false;
                }
                return d.dtype.and_then(|t| c.first_tok(t)).is_some_and(|f| !matches!(c.text_of(f), "signed" | "unsigned" | "["));
            }
            true
        }
        _ => false,
    }
}

fn double_negative_flag(c: &Cst, out: &mut Sink) {
    let types = expr_types(c);
    let ix = c.index();
    for e in c.nodes(PrefixExpr) {
        let Some(op) = c.keyword(e).map(|k| c.text_of(k)) else { continue };
        if op != "!" && op != "~" {
            continue;
        }
        let Some(&operand) = kids(c, e).first() else { continue };
        let o = strip(c, operand);
        if op == "~" && !types.get(&(ix, o)).and_then(|t| t.integral).is_some_and(|(w, ..)| w == 1) {
            continue;
        }
        if !matches!(c.kind(o), NameExpr | MemberExpr) {
            continue;
        }
        let Some(d) = crate::g_port2::expr_decl(c, o) else { continue };
        let name = if c.kind(o) == MemberExpr { c.last_text(o).to_string() } else { c.tree.bare_text(o) };
        if !user_variable(c, &d) || !negative_name(&name) {
            continue;
        }
        let times = user_visits(c, e);
        if times == 0 || crate::g_design::in_bad_body(c, e) {
            continue;
        }
        let Some(t) = c.first_tok(e) else { continue };
        let msg = format!(
            "'{}' negates a negative flag ('{name}'), which readers have to invert twice. Name the flag for the positive case (en_*, has_*, use_*) so the test reads straight.",
            crate::g_random_bug::expr_text(c, with_parens(c, e))
        );
        for _ in 0..times {
            out.add("SV-RDB-006", "DoubleNegativeFlag", Severity::Info, msg.clone(), c.loc(t));
        }
    }
}
