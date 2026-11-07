//! Rules ported in slice 4 that the group ports left: SV-API-001 (ref
//! argument never written), SV-PRF-006 (string-keyed associative array)
//! and UVM-FAC-001 (component created with new). Messages are verbatim.

use sv_syntax::{NodeId, SyntaxKind};

use crate::g_design_class::{type_name, written_names, Classes};
use crate::g_design_metrics::user_routines;
use crate::g_lang::ast_times;
use crate::syntax::Cst;
use crate::Check::Syntax;
use crate::{Rule, Severity, Sink};
use SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-API-001", name: "RefArgumentNeverWritten", check: Syntax(ref_argument_never_written) },
        Rule { id: "SV-PRF-006", name: "StringKeyAssociativeArray", check: Syntax(string_key_assoc) },
        Rule { id: "UVM-FAC-001", name: "UvmComponentDirectNew", check: Syntax(component_direct_new) },
        Rule { id: "SV-OOP-002", name: "UncheckedDynamicCast", check: Syntax(unchecked_dynamic_cast) },
        Rule { id: "UVM-REP-009", name: "InfoAtNoneVerbosity", check: Syntax(info_at_none) },
        Rule { id: "SV-TYP-009", name: "ArrayLiteralSizeMismatch", check: Syntax(array_literal_size) },
        Rule { id: "SV-LANG-002", name: "BitSelectOutOfBounds", check: Syntax(bit_select_out_of_bounds) },
        Rule { id: "SV-RDB-002", name: "MagicNumber", check: Syntax(magic_number) },
        Rule { id: "SV-FSM-001", name: "NonExhaustiveEnumCase", check: Syntax(non_exhaustive_enum_case) },
    ]
}

// ---- SV-API-001 ----

/// The routine declaration whose name token is `tok`.
fn routine_decl(c: &Cst, tok: u32) -> Option<NodeId> {
    c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)).find(|&d| {
        c.tree.child(d, Name).and_then(|n| c.tree.token_bounds(n)).is_some_and(|(_, b)| b == tok)
    })
}

/// The name token of port `name` in a routine's port list.
fn port_tok(c: &Cst, decl: NodeId, name: &str) -> Option<u32> {
    let list = c.tree.child(decl, PortList)?;
    for p in c.tree.children(list).filter(|&p| c.kind(p) == Port) {
        for d in c.tree.children(p).filter(|&d| c.kind(d) == Declarator) {
            let nm = c.tree.child(d, Name)?;
            if c.tree.bare_text(nm) == name {
                return c.tree.token_bounds(nm).map(|(a, _)| a);
            }
        }
    }
    None
}

/// Non-const ref formals. As in slang, a formal that writes its own type
/// without a direction is an input again (`ref int q[$], int v`).
fn ref_ports(c: &Cst, decl: NodeId) -> Vec<String> {
    let Some(list) = c.tree.child(decl, PortList) else { return Vec::new() };
    let mut out = Vec::new();
    let mut is_ref = false;
    let mut is_const = false;
    for p in c.tree.children(list).filter(|&p| c.kind(p) == Port) {
        let toks: Vec<&str> = c.toks(p).map(|t| c.text_of(t)).collect();
        if let Some(d) = toks.iter().find(|t| matches!(**t, "input" | "output" | "inout" | "ref")) {
            is_ref = *d == "ref";
            is_const = toks.contains(&"const");
        } else if c.tree.child(p, DataType).is_some() {
            is_ref = false;
        }
        if !is_ref || is_const {
            continue;
        }
        for d in c.tree.children(p).filter(|&d| c.kind(d) == Declarator) {
            if let Some(nm) = c.tree.child(d, Name) {
                out.push(c.tree.bare_text(nm));
            }
        }
    }
    out
}

fn ref_argument_never_written(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    let g = crate::g_design_types::globals(c);
    for r in user_routines(c, &classes) {
        let Some(body) = r.body else { continue };
        let Some(decl) = routine_decl(c, r.tok) else { continue };
        if let Some(i) = r.class {
            let m = classes.list[i].methods.iter().find(|m| m.tok == r.tok);
            if m.is_some_and(|m| m.virtual_kw) || classes.override_of(i, &r.name).is_some() {
                continue;
            }
        }
        let refs = ref_ports(c, decl);
        if refs.is_empty() {
            continue;
        }
        let env = crate::g_design_types::Env::new(c, &classes, r.class, Some(decl), &g);
        let written = written_names(c, &env, body, true);
        for a in refs {
            if written.contains(&a) {
                continue;
            }
            let Some(tok) = port_tok(c, decl, &a) else { continue };
            for _ in 0..r.times {
                s.add(
                    "SV-API-001",
                    "RefArgumentNeverWritten",
                    Severity::Info,
                    format!(
                        "'{a}' is passed by ref to '{}' but never modified. Declare it 'const ref' so callers can see the routine only reads it.",
                        r.name
                    ),
                    c.loc(tok),
                );
            }
        }
    }
}

// ---- SV-PRF-006 ----

/// The first unpacked dimension is `[string]`. A typedef'd associative
/// type is an alias in slang, which the rule does not look through.
fn string_keyed(c: &Cst, dcl: NodeId) -> bool {
    c.tree.children(dcl).find(|&d| c.kind(d) == Dim).is_some_and(|d| {
        c.tree.children(d).count() == 1
            && c.tree.children(d).next().is_some_and(|k| c.kind(k) == DataType && c.tree.bare_text(k) == "string")
    })
}

fn string_key_assoc(c: &Cst, s: &mut Sink) {
    let decls = c.nodes(DataDecl).into_iter().chain(c.nodes(Port));
    for dd in decls {
        // Struct members are fields, not variables; module ports are nets.
        if c.kind(dd) == DataDecl && std::iter::successors(c.parent(dd), |&a| c.parent(a)).any(|a| c.kind(a) == DataType) {
            continue;
        }
        if c.kind(dd) == Port
            && !c.parent(dd).and_then(|l| c.parent(l)).is_some_and(|r| matches!(c.kind(r), FunctionDecl | TaskDecl))
        {
            continue;
        }
        for dcl in c.tree.children(dd).filter(|&d| c.kind(d) == Declarator) {
            if !string_keyed(c, dcl) {
                continue;
            }
            let Some(nm) = c.tree.child(dcl, Name) else { continue };
            let Some((tok, _)) = c.tree.token_bounds(nm) else { continue };
            let name = c.tree.bare_text(nm);
            for _ in 0..symbol_times(c, dd) {
                s.add(
                    "SV-PRF-006",
                    "StringKeyAssociativeArray",
                    Severity::Warning,
                    format!(
                        "Associative array '{name}' is indexed by 'string'. Associative string lookups incur dynamic hashing and heap memory allocations on every element access, severely degrading simulation performance in hot transaction paths. Use an integral type, enum, or integer ID instead."
                    ),
                    c.loc(tok),
                );
            }
        }
    }
}

/// How often a symbol visitor from the root reaches a declaration: once per
/// instance of its module or specialization of its class.
fn symbol_times(c: &Cst, n: NodeId) -> usize {
    crate::g_lang::units(c).times(c, n)
}

// ---- UVM-FAC-001 ----

/// A class type as slang prints it: `name#(args)`, without a package.
fn class_type_text(c: &Cst, dt: NodeId) -> String {
    let text: String = c.toks(dt).map(|t| c.text_of(t)).collect();
    let head = text.split('#').next().unwrap_or("");
    match head.rfind("::") {
        Some(p) => text[p + 2..].to_string(),
        None => text,
    }
}

fn component_direct_new(c: &Cst, s: &mut Sink) {
    let classes = Classes::new(c);
    let g = crate::g_design_types::globals(c);
    let is_component = |i: usize| classes.chain(i).iter().any(|(n, _)| n == "uvm_component");
    for nx in c.nodes(NewExpr) {
        // `new[n]` makes an array; `new obj` copies.
        if c.tok(nx, "[").is_some() || (c.tree.child(nx, ArgList).is_none() && c.tree.children(nx).next().is_some()) {
            continue;
        }
        let Some(p) = c.parent(nx) else { continue };
        let target = match c.kind(p) {
            BinaryExpr if c.tok(p, "=").is_some() && c.tree.children(p).nth(1) == Some(nx) => {
                let Some(l) = c.tree.children(p).next() else { continue };
                let cls = classes.enclosing(c, nx);
                let routine = crate::g_design_class::routine_of(c, nx);
                let env = crate::g_design_types::Env::new(c, &classes, cls, routine, &g);
                let k = env.expr_type(l).filter(|t| !t.array).and_then(|t| t.class);
                k.map(|k| (k, env.decl_type(l).map(|t| class_type_text(c, t)).unwrap_or_else(|| classes.list[k].name.clone())))
            }
            Declarator => {
                let Some(dt) = c.parent(p).and_then(|d| c.tree.child(d, DataType)) else { continue };
                type_name(c, dt).and_then(|n| classes.find(&n).map(|k| (k, class_type_text(c, dt))))
            }
            _ => None,
        };
        let Some((k, name)) = target else { continue };
        if !is_component(k) {
            continue;
        }
        let times = ast_times(c, nx);
        for _ in 0..times {
            s.add(
                "UVM-FAC-001",
                "UvmComponentDirectNew",
                Severity::Error,
                format!(
                    "Direct instantiation of uvm_component '{name}' using new(). Use type_id::create() to enable factory overrides."
                ),
                c.loc_of(nx),
            );
        }
    }
}

// ---- SV-OOP-002 ----

fn unchecked_dynamic_cast(c: &Cst, s: &mut Sink) {
    for st in c.nodes(ExprStmt) {
        let Some(mut e) = c.tree.children(st).next() else { continue };
        // `void'($cast(a, b))`
        if c.kind(e) == CastExpr && c.tree.children(e).next().is_some_and(|t| c.tree.bare_text(t) == "void") {
            let Some(x) = c.tree.children(e).nth(1) else { continue };
            e = x;
        }
        if c.kind(e) != CallExpr || crate::g_random::call_name(c, e) != Some("$cast") {
            continue;
        }
        for _ in 0..ast_times(c, e) {
            s.add(
                "SV-OOP-002",
                "UncheckedDynamicCast",
                Severity::Warning,
                "Return value of $cast() is ignored or cast to void. If dynamic cast fails, destination handle remains null, risking runtime null-pointer dereferences."
                    .to_string(),
                c.loc_of(st),
            );
        }
    }
}

// ---- UVM-REP-009 ----

fn info_at_none(c: &Cst, s: &mut Sink) {
    for call in c.nodes(CallExpr) {
        if crate::g_random::call_name(c, call) != Some("uvm_report_info") {
            continue;
        }
        let args = c.call_args(call);
        let Some(v) = c.arg_expr(args.get(2).copied().flatten()) else { continue };
        let v = c.strip(v);
        let none = match c.kind(v) {
            NameExpr | ScopedExpr => c.last_text(v) == "UVM_NONE",
            Literal => {
                let t: String = c.toks(v).map(|t| c.text_of(t)).collect();
                let digits = t.rsplit(['d', 'D', 'h', 'H', 'b', 'B', 'o', 'O', '\'']).next().unwrap_or("");
                !digits.is_empty() && digits.chars().all(|ch| ch == '0' || ch == '_')
            }
            _ => false,
        };
        if !none {
            continue;
        }
        let Some(first) = c.first_tok(call) else { continue };
        let loc = c.loc_at(c.tree.tokens[first as usize].origin);
        for _ in 0..ast_times(c, call) {
            s.add(
                "UVM-REP-009",
                "InfoAtNoneVerbosity",
                Severity::Info,
                "Informational message at UVM_NONE: it prints at every verbosity, +UVM_VERBOSITY cannot silence it, and it floods regression logs. Use UVM_LOW for messages that should normally show, and keep UVM_NONE for the few lines every run must print (a test's final verdict)."
                    .to_string(),
                loc,
            );
        }
    }
}

// ---- SV-TYP-009 ----

/// slang's WrongNumberAssignmentPatterns diagnostic, once per pattern.
fn array_literal_size(c: &Cst, s: &mut Sink) {
    for ap in c.nodes(AssignPattern) {
        if !crate::g_random_sema::pattern_count_mismatch(c, ap) {
            continue;
        }
        s.add(
            "SV-TYP-009",
            "ArrayLiteralSizeMismatch",
            Severity::Warning,
            "Array literal assignment pattern element count does not match target array dimension. In IEEE 1800 \u{a7} 10.9 array assignment semantics, positional assignment patterns must provide an element count matching the target array dimension."
                .to_string(),
            c.loc_of(ap),
        );
    }
}

// ---- SV-LANG-002 ----

/// The declaration a name or member expression refers to.
fn decl_of_expr(c: &Cst, e: NodeId) -> Option<(crate::g_c_support::Decl, String)> {
    use crate::g_c_support::{class_member, expr_type, find_class, lookup};
    match c.kind(e) {
        NameExpr if c.tree.children(e).next().is_none() => {
            let name = c.tree.bare_text(e);
            lookup(c, e, &name).map(|d| (d, name))
        }
        MemberExpr => {
            let base = c.tree.children(e).next()?;
            let member = c.last_text(e).to_string();
            let cls = find_class(c, &expr_type(c, base)?, e)?;
            class_member(c, cls, &member).map(|d| (d, member))
        }
        _ => None,
    }
}

fn int_value(c: &Cst, n: NodeId) -> Option<i64> {
    let n = c.strip(n);
    if c.kind(n) != Literal {
        return None;
    }
    let toks: Vec<u32> = c.toks(n).collect();
    let [t] = toks[..] else { return None };
    let text = c.text_of(t).replace('_', "");
    text.bytes().all(|b| b.is_ascii_digit()).then(|| text.parse().ok()).flatten()
}

/// `[left:right]` of the first dimension a select on the declaration
/// indexes: its first unpacked dimension, else its first packed one, else
/// the bits of a built-in integer type.
fn fixed_range(c: &Cst, d: &crate::g_c_support::Decl) -> Option<(i64, i64)> {
    let bounds = |dim: NodeId| -> Option<(i64, i64)> {
        let k: Vec<NodeId> = c.tree.children(dim).collect();
        match k[..] {
            [n] if c.tok(dim, ":").is_none() => {
                let n = int_value(c, n)?;
                (n > 0).then_some((0, n - 1))
            }
            [a, b] if c.tok(dim, ":").is_some() && c.tok(dim, "+:").is_none() && c.tok(dim, "-:").is_none() => {
                Some((int_value(c, a)?, int_value(c, b)?))
            }
            _ => None,
        }
    };
    if c.kind(d.node) == Declarator {
        if let Some(dim) = c.tree.children(d.node).find(|&x| c.kind(x) == Dim) {
            return bounds(dim);
        }
    }
    let mut dt = d.dtype?;
    // `typedef logic [3:0] nib_t;`, `typedef int arr_t[4];`
    for _ in 0..4 {
        let toks: Vec<u32> = c.toks(dt).collect();
        let [t] = toks[..] else { break };
        if c.tree.tokens[t as usize].kind != sv_syntax::TokenKind::Identifier {
            break;
        }
        let name = c.text_of(t);
        let Some(td) = c.nodes(TypedefDecl).into_iter().find(|&td| c.tree.child(td, Name).is_some_and(|n| c.tree.bare_text(n) == name))
        else {
            return None;
        };
        if let Some(dim) = c.tree.children(td).find(|&x| c.kind(x) == Dim) {
            return bounds(dim);
        }
        dt = c.tree.child(td, DataType)?;
    }
    if let Some(dim) = c.tree.children(dt).find(|&x| c.kind(x) == Dim) {
        return bounds(dim);
    }
    let kw = c.first_tok(dt).map(|t| c.text_of(t))?;
    let bits = match kw {
        "byte" => 8,
        "shortint" => 16,
        "int" | "integer" => 32,
        "longint" => 64,
        _ => return None,
    };
    // Only the keyword (and signing) is written.
    if c.toks(dt).count() > 2 {
        return None;
    }
    Some((bits - 1, 0))
}

fn bit_select_out_of_bounds(c: &Cst, s: &mut Sink) {
    for sel in c.nodes(SelectExpr) {
        if c.tok(sel, ":").is_some() || c.tok(sel, "+:").is_some() || c.tok(sel, "-:").is_some() {
            continue;
        }
        let k: Vec<NodeId> = c.tree.children(sel).collect();
        let [base, index] = k[..] else { continue };
        let Some(idx) = int_value(c, index) else { continue };
        let Some((d, name)) = decl_of_expr(c, c.strip(base)) else { continue };
        let Some((left, right)) = fixed_range(c, &d) else { continue };
        if idx >= left.min(right) && idx <= left.max(right) {
            continue;
        }
        for _ in 0..ast_times(c, sel) {
            s.add(
                "SV-LANG-002",
                "BitSelectOutOfBounds",
                Severity::Error,
                format!(
                    "Static out-of-bounds bit/element select: index [{idx}] is outside declared range [{left}:{right}] of '{name}'. In SystemVerilog (IEEE 1800 \u{a7} 7.4.5), reading out-of-bounds returns 'x without crashing, silently propagating uninitialized state into downstream verification logic."
                ),
                c.loc_of(sel),
            );
        }
    }
}

// ---- SV-RDB-002 ----

/// An integer literal's value, when it has no x/z bits and fits.
fn literal_value(c: &Cst, n: NodeId) -> Option<i64> {
    let toks: Vec<u32> = c.toks(n).collect();
    let texts: Vec<&str> = toks.iter().map(|&t| c.text_of(t)).collect();
    let (base, digits) = match texts[..] {
        [d] if d.bytes().all(|b| b.is_ascii_digit() || b == b'_') => (10, d),
        [b, d] | [_, b, d] if b.starts_with('\'') => {
            let radix = match b.trim_start_matches('\'').trim_start_matches(['s', 'S']).chars().next()? {
                'd' | 'D' => 10,
                'h' | 'H' => 16,
                'o' | 'O' => 8,
                'b' | 'B' => 2,
                _ => return None,
            };
            (radix, d)
        }
        _ => return None,
    };
    let d: String = digits.chars().filter(|&ch| ch != '_').collect();
    i64::from_str_radix(&d, base).ok()
}

fn is_display_call(n: &str) -> bool {
    n.starts_with("$display")
        || n.starts_with("$write")
        || n.starts_with("$sformat")
        || n.starts_with("$fdisplay")
        || n.starts_with("$fwrite")
        || n == "$psprintf"
        || n == "$swrite"
}

fn magic_number(c: &Cst, s: &mut Sink) {
    use crate::g_design_class::{report_level, Level, Role};
    let classes = Classes::new(c);
    let in_module = |n: NodeId| {
        std::iter::successors(c.parent(n), |&a| c.parent(a))
            .any(|a| matches!(c.kind(a), ModuleDecl | InterfaceDecl | ProgramDecl | CheckerDecl))
    };
    // Default specializations join only when the model has a class at all.
    let any_plain = classes.list.iter().any(|k| !k.generic && !in_module(k.node));
    for i in classes.user() {
        let cls = &classes.list[i];
        if cls.generic && !any_plain {
            continue;
        }
        // The model indexes classes of packages and $unit, not of modules.
        if classes.role(i) == Role::RegModel
            || in_module(cls.node)
        {
            continue;
        }
        for m in &cls.methods {
            let Some(body) = m.body else { continue };
            let where_ = format!("{}::{}", cls.name, m.name);
            let mut reported = std::collections::HashSet::new();
            let mut stack = vec![body];
            while let Some(x) = stack.pop() {
                match c.kind(x) {
                    CallExpr => {
                        let name = crate::g_random::call_name(c, x).unwrap_or("");
                        if report_level(c, x) != Level::None || is_display_call(name) {
                            continue;
                        }
                    }
                    TimingControl if c.tok(x, "#").is_some() => continue,
                    // Local declarations are symbols the statement visitor
                    // does not enter.
                    DataDecl => continue,
                    Literal => {
                        let Some(v) = literal_value(c, x) else { continue };
                        if matches!(v, 0 | 1 | -1) {
                            continue;
                        }
                        let Some(t) = c.first_tok(x) else { continue };
                        if c.tree.tokens[t as usize].from_macro || !crate::g_random_sema::ast_ok(c, x) {
                            continue;
                        }
                        if !reported.insert(v) {
                            continue;
                        }
                        s.add(
                            "SV-RDB-002",
                            "MagicNumber",
                            Severity::Info,
                            format!(
                                "Magic number {v} in {where_}. Give it a name (localparam, enum or config field) so its meaning and every place that depends on it are visible."
                            ),
                            c.loc(t),
                        );
                        continue;
                    }
                    _ => {}
                }
                let kids: Vec<NodeId> = c.tree.children(x).collect();
                stack.extend(kids.into_iter().rev());
            }
        }
    }
}

// ---- SV-FSM-001 ----

/// The enum a declaration's type is, through typedefs: its name as slang
/// prints the canonical type, and its members with their values.
fn enum_of(c: &Cst, dt: NodeId) -> Option<(String, Vec<(String, Option<i64>)>)> {
    let mut dt = dt;
    let mut name = String::new();
    for _ in 0..4 {
        if c.tok(dt, "enum").is_some() {
            let mut members = Vec::new();
            let mut next = Some(0i64);
            for d in c.tree.children(dt).filter(|&d| c.kind(d) == Declarator) {
                let Some(nm) = c.tree.child(d, Name) else { continue };
                let v = match c.tree.children(d).find(|&x| x != nm && c.kind(x) != Dim) {
                    Some(e) => literal_value(c, c.strip(e)),
                    None => next,
                };
                members.push((c.tree.bare_text(nm), v));
                next = v.map(|v| v + 1);
            }
            return Some((name, members));
        }
        let toks: Vec<u32> = c.toks(dt).collect();
        let [t] = toks[..] else { return None };
        let tn = c.text_of(t);
        let td = c.nodes(TypedefDecl).into_iter().find(|&td| c.tree.child(td, Name).is_some_and(|n| c.tree.bare_text(n) == tn))?;
        name = tn.to_string();
        dt = c.tree.child(td, DataType)?;
    }
    None
}

fn non_exhaustive_enum_case(c: &Cst, s: &mut Sink) {
    for cs in c.nodes(CaseStmt) {
        if !c.toks(cs).any(|t| matches!(c.text_of(t), "case" | "casex" | "casez")) {
            continue;
        }
        let parts = crate::g_meth_body::case_parts(c, cs);
        let Some(e) = parts.expr else { continue };
        let e = c.strip(e);
        let dtype = if c.kind(e) == CallExpr {
            // `case (get_endian())`: the routine's return type.
            crate::g_c_support::callee_decl(c, e)
                .and_then(|r| c.tree.child(r, ReturnType))
                .and_then(|r| c.tree.child(r, DataType))
        } else {
            let Some((d, _)) = decl_of_expr(c, e) else { continue };
            if c.kind(d.node) == Declarator && c.tree.children(d.node).any(|x| c.kind(x) == Dim) {
                continue;
            }
            d.dtype
        };
        let Some((name, members)) = dtype.and_then(|t| enum_of(c, t)) else { continue };
        if !parts.items.iter().any(|(_, _, def)| *def) {
            continue;
        }
        let mut handled = std::collections::HashSet::new();
        let mut labels_ok = true;
        for (labels, _, _) in &parts.items {
            for &l in labels {
                let l = c.strip(l);
                if c.kind(l) == NameExpr {
                    handled.insert(c.tree.bare_text(l));
                } else {
                    // A constant label compares the enum's value with an
                    // int; uvmcheck reports nothing for such a case.
                    labels_ok = false;
                }
            }
        }
        if !labels_ok {
            continue;
        }
        let names: Vec<&str> = members.iter().map(|(m, _)| m.as_str()).collect();
        let Some(order) = crate::g_c_support::std_unordered_order(&names) else { continue };
        let missing: Vec<String> = order.into_iter().filter(|m| !handled.contains(m)).map(|m| format!("'{m}'")).collect();
        if missing.is_empty() {
            continue;
        }
        for _ in 0..ast_times(c, cs) {
            s.add(
                "SV-FSM-001",
                "NonExhaustiveEnumCase",
                Severity::Warning,
                format!(
                    "Case statement switching over enum '{name}' does not explicitly handle variant(s): {}. Using a catch-all 'default:' arm silently masks unhandled states and allows specification drift bugs to pass undetected. Explicitly enumerate all variants or remove 'default:' to enforce full pattern exhaustiveness.",
                    missing.join(", ")
                ),
                c.loc_of(cs),
            );
        }
    }
}
