//! Group C of builder slice N6.3: the assertion and coverage rules. See
//! [`crate::g_concurrency`] for how the AST rules read the syntax tree.

use std::collections::{BTreeSet, HashMap, HashSet};

use dv_model::Loc;
use sv_syntax::{NodeId, SyntaxKind};

use crate::g_c_support::*;
use crate::g_concurrency::{is_stmt, start, written};
use crate::syntax::Cst;
use crate::Check::Syntax;
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-COV-001", name: "UnreferencedCovergroup", check: Syntax(unreferenced_covergroup) },
        Rule { id: "SV-COV-002", name: "ZeroWeightCoverpoint", check: Syntax(zero_weight) },
        Rule { id: "SV-COV-004", name: "IllegalBinOverlap", check: Syntax(illegal_bin_overlap) },
        Rule { id: "SV-COV-005", name: "UnboundedTransitionBins", check: Syntax(unbounded_transition) },
        Rule { id: "SV-COV-011", name: "AtLeastZeroOrNegative", check: Syntax(at_least_zero) },
        Rule { id: "SV-COV-013", name: "IgnoreBinsOverlapsIllegalBins", check: Syntax(ignore_illegal_overlap) },
        Rule { id: "SV-COV-016", name: "OverlappingRangesInCoverpointBin", check: Syntax(overlapping_ranges) },
        Rule { id: "SV-COV-017", name: "UnreachableCoverBin", check: Syntax(unreachable_bin) },
        Rule { id: "SV-SVA-001", name: "ImmediateAssertInCombinatorialLogic", check: Syntax(immediate_in_comb) },
        Rule { id: "SV-SVA-002", name: "DisableIffComplexExpression", check: Syntax(disable_iff_complex) },
        Rule { id: "SV-SVA-003", name: "MixedClockEdgesInProperty", check: Syntax(mixed_edges) },
        Rule { id: "SV-SVA-005", name: "MultiClockConcurrentAssertionDomain", check: Syntax(multi_clock) },
        Rule { id: "SVA-ACT-001", name: "AssertWithoutElse", check: Syntax(assert_without_else) },
        Rule { id: "SVA-ACT-002", name: "TimeInAssertionAction", check: Syntax(time_in_action) },
        Rule { id: "SVA-ACT-003", name: "SideEffectInAssertionAction", check: Syntax(side_effect_in_action) },
        Rule { id: "SVA-CTX-001", name: "ConcurrentAssertInFunction", check: Syntax(concurrent_in_function) },
    ]
}

fn add_n(s: &mut Sink, n: usize, rule: &'static str, name: &'static str, sev: Severity, msg: String, loc: Loc) {
    for _ in 0..n {
        s.add(rule, name, sev, msg.clone(), loc);
    }
}

fn label_of(c: &Cst, n: NodeId) -> Option<String> {
    let l = c.tree.child(n, StmtLabel)?;
    c.toks(l).next().map(|t| c.text_of(t).to_string())
}

// ---- coverage helpers ----

/// A bins declaration: its keyword (`bins`, `illegal_bins`, `ignore_bins`)
/// and name.
fn bins_kw<'a>(c: &'a Cst, b: NodeId) -> &'a str {
    c.toks(b).map(|t| c.text_of(t)).find(|t| matches!(*t, "bins" | "illegal_bins" | "ignore_bins")).unwrap_or("")
}

fn bins_name(c: &Cst, b: NodeId) -> Option<(String, u32)> {
    let n = c.tree.child(b, Name)?;
    let t = c.first_tok(n)?;
    Some((c.text_of(t).to_string(), t))
}

/// The values of a `{ ... }` bins initializer.
fn bins_values(c: &Cst, b: NodeId) -> Vec<NodeId> {
    match c.tree.child(b, RangeList) {
        Some(r) => kids(c, r),
        None => Vec::new(),
    }
}

/// `option.x = v` of a coverage option: the assignment.
fn option_parts(c: &Cst, o: NodeId) -> Option<(NodeId, NodeId)> {
    let e = *kids(c, o).first()?;
    if c.kind(e) != BinaryExpr || !c.keyword(e).is_some_and(|t| c.text_of(t) == "=") {
        return None;
    }
    let k = kids(c, e);
    Some((e, *k.get(1)?))
}

/// What a coverage option belongs to: its coverpoint's or cross's label (or
/// `unnamed_cp` / `unnamed_cross`), else its covergroup's name.
fn option_target(c: &Cst, o: NodeId) -> String {
    for a in ancestors(c, o) {
        match c.kind(a) {
            Coverpoint => return label_of(c, a).unwrap_or_else(|| "unnamed_cp".into()),
            Cross => return label_of(c, a).unwrap_or_else(|| "unnamed_cross".into()),
            CovergroupDecl => return decl_name(c, a),
            _ => {}
        }
    }
    String::new()
}

// ---- SV-COV-001 ----

fn unreferenced_covergroup(c: &Cst, s: &mut Sink) {
    // Declared: covergroups of the model's classes.
    let mut declared = Vec::new();
    for cls in model_classes(c) {
        for g in kids(c, cls) {
            if c.kind(g) != CovergroupDecl {
                continue;
            }
            let Some(nm) = c.tree.child(g, Name) else { continue };
            let Some(t) = c.first_tok(nm) else { continue };
            let has_event = c.tree.child(g, TimingControl).is_some() || c.tree.child(g, BlockEvent).is_some();
            declared.push((c.text_of(t).to_string(), decl_name(c, cls), c.loc(t), has_event));
        }
    }
    if declared.is_empty() {
        return;
    }
    let covergroups: HashSet<String> = c.nodes(CovergroupDecl).into_iter().map(|g| decl_name(c, g)).collect();
    let mut inst: HashSet<String> = HashSet::new();
    let mut sampled: HashSet<String> = HashSet::new();
    let strip = |t: String| -> String { t.chars().filter(|ch| !ch.is_whitespace()).collect() };
    for n in c.nodes(NewExpr) {
        let Some(p) = c.parent(n) else { continue };
        // The covergroup type the `new` makes.
        let (ty, lhs) = match c.kind(p) {
            BinaryExpr if c.keyword(p).is_some_and(|t| c.text_of(t) == "=") && kids(c, p).get(1) == Some(&n) => {
                let l = kids(c, p)[0];
                let text = strip(slang_text(c, l));
                let name = text.strip_prefix("this.").unwrap_or(&text).to_string();
                let ty = if covergroups.contains(&name) && !name.contains('.') {
                    Some(name.clone())
                } else {
                    expr_type(c, l).filter(|t| covergroups.contains(t))
                };
                (ty, Some(text))
            }
            Declarator => {
                let ty = c.parent(p).and_then(|d| c.tree.child(d, DataType)).and_then(|t| c.first_tok(t)).map(|t| c.text_of(t).to_string());
                (ty.filter(|t| covergroups.contains(t)), None)
            }
            _ => (None, None),
        };
        let Some(ty) = ty else { continue };
        if let Some(text) = lhs {
            if let Some(x) = text.strip_prefix("this.") {
                inst.insert(x.to_string());
            }
            inst.insert(text);
        }
        inst.insert(ty);
    }
    for call in c.nodes(CallExpr) {
        let Some((callee, name, _)) = call_parts(c, call) else { continue };
        if name != "sample" {
            continue;
        }
        let text = slang_text(c, callee);
        if let Some(dot) = text.rfind('.') {
            let obj = strip(text[..dot].to_string());
            if let Some(x) = obj.strip_prefix("this.") {
                sampled.insert(x.to_string());
            }
            sampled.insert(obj);
        }
    }
    for (name, cls, loc, has_event) in declared {
        if !inst.contains(&name) {
            s.add(
                "SV-COV-001",
                "UnreferencedCovergroup",
                Severity::Warning,
                format!(
                    "Covergroup '{name}' is declared in class '{cls}' but is never instantiated with 'new()'. Uninstantiated covergroups collect 0 coverage data, \
                     masking unverified functional requirements."
                ),
                loc,
            );
        } else if !has_event && !sampled.contains(&name) {
            s.add(
                "SV-COV-001",
                "UnreferencedCovergroup",
                Severity::Warning,
                format!(
                    "Covergroup '{name}' in class '{cls}' is instantiated but has no clocking event and is never sampled via '{name}.sample()'. It will remain at 0% coverage throughout simulation."
                ),
                loc,
            );
        }
    }
}

// ---- SV-COV-002 / SV-COV-011 ----

fn zero_weight(c: &Cst, s: &mut Sink) {
    for o in c.nodes(CoverageOption) {
        let Some((e, rhs)) = option_parts(c, o) else { continue };
        let text = slang_text(c, e);
        if !text.contains("weight") {
            continue;
        }
        let r = slang_trimmed(c, rhs);
        if !matches!(r.as_str(), "0" | "'0" | "32'd0" | "32'b0") {
            continue;
        }
        let target = option_target(c, o);
        let Some(loc) = start(c, o) else { continue };
        s.add(
            "SV-COV-002",
            "ZeroWeightCoverpoint",
            Severity::Warning,
            format!(
                "Coverage construct '{target}' specifies zero weight ('{text}'). \
                 Setting option.weight or type_option.weight to 0 causes this construct to contribute 0% to regression coverage calculation, masking untested logic."
            ),
            loc,
        );
    }
}

fn at_least_zero(c: &Cst, s: &mut Sink) {
    for o in c.nodes(CoverageOption) {
        let Some((e, rhs)) = option_parts(c, o) else { continue };
        if !slang_text(c, e).contains("at_least") {
            continue;
        }
        let r = slang_trimmed(c, rhs);
        if !(matches!(r.as_str(), "0" | "'0" | "32'd0" | "32'b0") || r.starts_with('-')) {
            continue;
        }
        let target = option_target(c, o);
        let Some(loc) = start(c, o) else { continue };
        s.add(
            "SV-COV-011",
            "AtLeastZeroOrNegative",
            Severity::Warning,
            format!(
                "Coverage construct '{target}' specifies non-positive 'at_least' threshold ('{r}'). In IEEE 1800 § 19.5, 'option.at_least' and 'type_option.at_least' define \
                 the minimum number of hit occurrences required for a bin to be considered covered and must be an integer >= 1. \
                 Setting at_least <= 0 causes bins to be trivially covered without verification or invalidates coverage metrics."
            ),
            loc,
        );
    }
}

// ---- SV-COV-004 ----

fn illegal_bin_overlap(c: &Cst, s: &mut Sink) {
    for cp in c.nodes(Coverpoint) {
        let cp_name = label_of(c, cp).unwrap_or_else(|| "unnamed_cp".into());
        let mut regular = Vec::new();
        let mut special = Vec::new();
        for b in kids(c, cp) {
            if c.kind(b) != Bins {
                continue;
            }
            let kw = bins_kw(c, b).to_string();
            let name = bins_name(c, b).map(|x| x.0).unwrap_or_else(|| "unnamed".into());
            let Some(loc) = start(c, b) else { continue };
            let vals: Vec<String> = bins_values(c, b).into_iter().map(|v| slang_trimmed(c, v)).filter(|v| !v.is_empty()).collect();
            let Some(vals) = std_unordered_order(&vals) else { continue };
            if kw == "bins" {
                regular.push((name, vals));
            } else if kw == "illegal_bins" || kw == "ignore_bins" {
                special.push((name, kw, vals, loc));
            }
        }
        for (sname, skw, svals, sloc) in &special {
            for (rname, rvals) in &regular {
                if let Some(v) = svals.iter().find(|v| rvals.contains(v)) {
                    s.add(
                        "SV-COV-004",
                        "IllegalBinOverlap",
                        Severity::Error,
                        format!(
                            "Coverpoint '{cp_name}': overlapping value '{v}' detected between regular bins '{rname}' and {skw} '{sname}'. In SystemVerilog (IEEE 1800 § 19.5), regular bins must not overlap with illegal_bins or ignore_bins, creating non-deterministic bin sampling and simulator errors."
                        ),
                        *sloc,
                    );
                }
            }
        }
    }
}

// ---- SV-COV-005 ----

fn unbounded_transition(c: &Cst, s: &mut Sink) {
    for it in c.nodes(TransItem) {
        let toks: Vec<u32> = c.toks(it).collect();
        let Some(open) = toks.iter().position(|&t| c.text_of(t) == "[") else { continue };
        let Some(close) = toks.iter().rposition(|&t| c.text_of(t) == "]") else { continue };
        let text = span_text(c, toks[open], toks[close]);
        if !text.contains('$') {
            continue;
        }
        let bin = ancestors(c, it).find(|&a| c.kind(a) == Bins).and_then(|b| bins_name(c, b)).map(|x| x.0).unwrap_or_else(|| "unnamed_bin".into());
        s.add(
            "SV-COV-005",
            "UnboundedTransitionBins",
            Severity::Warning,
            format!(
                "Coverage transition bin '{bin}' contains an unbounded repetition sequence '{text}'. Unbounded transition sequences ([*1:$], [->1:$]) require simulator coverage engines to track open threads indefinitely, triggering severe memory bloat and simulator slowness. Use a fixed upper bound."
            ),
            c.loc(toks[open]),
        );
    }
}

// ---- SV-COV-013 ----

fn ignore_illegal_overlap(c: &Cst, s: &mut Sink) {
    for cp in c.nodes(Coverpoint) {
        let cp_name = label_of(c, cp).unwrap_or_else(|| "<unnamed_coverpoint>".into());
        let mut ignore: HashMap<String, (String, Loc)> = HashMap::new();
        let mut illegal: Vec<(String, (String, Loc))> = Vec::new();
        for b in kids(c, cp) {
            if c.kind(b) != Bins {
                continue;
            }
            let kw = bins_kw(c, b);
            let name = bins_name(c, b).map(|x| x.0).unwrap_or_default();
            for v in bins_values(c, b) {
                let Some(t) = c.first_tok(v) else { continue };
                let mut val = c.text_of(t).trim_start_matches('\\').to_string();
                if val.is_empty() {
                    val = slang_text(c, v);
                }
                let info = (name.clone(), c.loc(t));
                if kw == "ignore_bins" {
                    ignore.insert(val, info);
                } else if kw == "illegal_bins" {
                    match illegal.iter_mut().find(|x| x.0 == val) {
                        Some(x) => x.1 = info,
                        None => illegal.push((val, info)),
                    }
                }
            }
        }
        for (val, (ill, loc)) in illegal {
            if let Some((ign, _)) = ignore.get(&val) {
                s.add(
                    "SV-COV-013",
                    "IgnoreBinsOverlapsIllegalBins",
                    Severity::Error,
                    format!(
                        "Coverage value '{val}' is specified in both 'ignore_bins' ({ign}) and 'illegal_bins' ({ill}) within coverpoint '{cp_name}'. \
                         In IEEE 1800 § 19.5 coverage methodology, overlapping \
                         ignore and illegal bins creates contradictory coverage semantics and causes simulator runtime errors."
                    ),
                    loc,
                );
            }
        }
    }
}

// ---- SV-COV-016 ----

/// `std::stoll(text, nullptr, 0)` of a decimal literal as written.
fn stoll0(text: &str) -> Option<i64> {
    let b = text.as_bytes();
    if b.is_empty() || !b[0].is_ascii_digit() {
        return None;
    }
    let (base, digits) = if b[0] == b'0' && b.len() > 1 && (b[1] == b'x' || b[1] == b'X') {
        (16, &text[2..])
    } else if b[0] == b'0' {
        (8, text)
    } else {
        (10, text)
    };
    let mut v: i64 = 0;
    let mut any = false;
    for ch in digits.chars() {
        let Some(d) = ch.to_digit(base) else { break };
        v = v.checked_mul(base as i64)?.checked_add(d as i64)?;
        any = true;
    }
    any.then_some(v)
}

/// slang's `IntegerLiteralExpression`: an unsized decimal number.
fn int_literal(c: &Cst, e: NodeId) -> Option<i64> {
    if c.kind(e) != Literal {
        return None;
    }
    let toks: Vec<u32> = c.toks(e).collect();
    if toks.len() != 1 {
        return None;
    }
    let t = c.text_of(toks[0]);
    if !t.bytes().all(|ch| ch.is_ascii_digit() || ch == b'_') {
        return None;
    }
    stoll0(t)
}

fn overlapping_ranges(c: &Cst, s: &mut Sink) {
    for b in c.nodes(Bins) {
        let mut ranges = Vec::new();
        for r in subtree(c, b) {
            if c.kind(r) != OpenRange {
                continue;
            }
            let k = kids(c, r);
            if k.len() != 2 {
                continue;
            }
            if let (Some(lo), Some(hi)) = (int_literal(c, k[0]), int_literal(c, k[1])) {
                if lo <= hi {
                    ranges.push((lo, hi));
                }
            }
        }
        'found: for i in 0..ranges.len() {
            for j in i + 1..ranges.len() {
                let (a, b2) = (ranges[i], ranges[j]);
                if a.0.max(b2.0) <= a.1.min(b2.1) {
                    let Some((name, t)) = bins_name(c, b) else { break 'found };
                    s.add(
                        "SV-COV-016",
                        "OverlappingRangesInCoverpointBin",
                        Severity::Warning,
                        format!(
                            "Coverpoint bin '{name}' contains overlapping value ranges '[{}:{}]' and '[{}:{}]'. In functional coverage \
                             methodology (IEEE 1800 § 19.5), overlapping sub-ranges within a single \
                             bin definition are redundant and indicate specification errors in the functional coverage model.",
                            a.0, a.1, b2.0, b2.1
                        ),
                        c.loc(t),
                    );
                    break 'found;
                }
            }
        }
    }
}

// ---- SV-COV-017 ----

/// Width and signedness of a declared integral type, as written.
fn type_width(c: &Cst, dt: NodeId) -> Option<(u32, bool)> {
    let toks: Vec<&str> = c.toks(dt).map(|t| c.text_of(t)).collect();
    let base = *toks.first()?;
    let explicit_signed = toks.contains(&"signed");
    let explicit_unsigned = toks.contains(&"unsigned");
    let (w, signed) = match base {
        "bit" | "logic" | "reg" => (1, false),
        "byte" => (8, true),
        "shortint" => (16, true),
        "int" | "integer" => (32, true),
        "longint" => (64, true),
        _ => return None,
    };
    let signed = (signed || explicit_signed) && !explicit_unsigned;
    let dims: Vec<NodeId> = c.tree.children(dt).filter(|&d| c.kind(d) == Dim).collect();
    if dims.is_empty() {
        return Some((w, signed));
    }
    if matches!(base, "byte" | "shortint" | "int" | "integer" | "longint") {
        return None;
    }
    let mut total = 1u32;
    for d in dims {
        let k = kids(c, d);
        if k.len() != 2 {
            return None;
        }
        let a = const_value(c, k[0], 0)?;
        let b = const_value(c, k[1], 0)?;
        total = total.checked_mul(((a - b).unsigned_abs() + 1) as u32)?;
    }
    Some((total, signed))
}

/// The value of a constant integer expression as written: literals,
/// negation, parameters and simple arithmetic.
fn const_value(c: &Cst, e: NodeId, depth: u32) -> Option<i128> {
    if depth > 16 {
        return None;
    }
    match c.kind(e) {
        Literal => literal_value(c, e),
        ParenExpr => const_value(c, *kids(c, e).first()?, depth + 1),
        PrefixExpr => {
            let op = c.keyword(e).map(|t| c.text_of(t))?;
            let v = const_value(c, *kids(c, e).first()?, depth + 1)?;
            match op {
                "-" => Some(-v),
                "+" => Some(v),
                _ => None,
            }
        }
        BinaryExpr => {
            let op = c.keyword(e).map(|t| c.text_of(t))?;
            let k = kids(c, e);
            let a = const_value(c, *k.first()?, depth + 1)?;
            let b = const_value(c, *k.get(1)?, depth + 1)?;
            match op {
                "+" => a.checked_add(b),
                "-" => a.checked_sub(b),
                "*" => a.checked_mul(b),
                _ => None,
            }
        }
        NameExpr => {
            let t = c.first_tok(e)?;
            let d = lookup(c, e, c.text_of(t))?;
            if d.kind != DeclKind::Parameter {
                return None;
            }
            let init = kids(c, d.node).into_iter().filter(|&k| c.kind(k) != Name).last()?;
            const_value(c, init, depth + 1)
        }
        _ => None,
    }
}

fn literal_value(c: &Cst, e: NodeId) -> Option<i128> {
    let text: String = c.tree.token_range(e).into_iter().map(|t| c.text_of(t)).collect::<String>().replace('_', "");
    if let Some(q) = text.find('\'') {
        let rest = &text[q + 1..];
        let rest = rest.strip_prefix(['s', 'S']).unwrap_or(rest);
        let (base, digits) = match rest.chars().next()? {
            'd' | 'D' => (10, &rest[1..]),
            'h' | 'H' => (16, &rest[1..]),
            'b' | 'B' => (2, &rest[1..]),
            'o' | 'O' => (8, &rest[1..]),
            _ => return None,
        };
        if digits.is_empty() || digits.chars().any(|ch| matches!(ch, 'x' | 'X' | 'z' | 'Z' | '?')) {
            return None;
        }
        return i128::from_str_radix(digits, base).ok();
    }
    if text.bytes().all(|b| b.is_ascii_digit()) && !text.is_empty() {
        return text.parse().ok();
    }
    None
}

fn unreachable_bin(c: &Cst, s: &mut Sink) {
    for cp in c.nodes(Coverpoint) {
        let Some(expr) = kids(c, cp).into_iter().find(|&k| !matches!(c.kind(k), StmtLabel | Attribute | Bins | CoverageOption)) else { continue };
        let cp_name = match label_of(c, cp) {
            Some(l) => l,
            None if c.kind(expr) == NameExpr => c.tree.bare_text(expr),
            None => String::new(),
        };
        let decl = match c.kind(expr) {
            NameExpr => c.first_tok(expr).and_then(|t| lookup(c, expr, c.text_of(t))),
            MemberExpr => {
                let base = kids(c, expr).first().copied();
                base.and_then(|b| expr_type(c, b)).and_then(|t| find_class(c, &t, expr)).and_then(|k| class_member(c, k, c.last_text(expr)))
            }
            _ => None,
        };
        let Some((width, signed)) = decl.and_then(|d| d.dtype).and_then(|dt| type_width(c, dt)) else { continue };
        if width == 0 || width > 62 {
            continue;
        }
        let span = 1i128 << width;
        let (lo_d, hi_d) = if signed { (-span / 2, span / 2 - 1) } else { (0, span - 1) };
        let n = times(c, cp);
        for b in kids(c, cp) {
            if c.kind(b) != Bins || bins_kw(c, b) != "bins" {
                continue;
            }
            let Some(rl) = c.tree.child(b, RangeList) else { continue };
            let vals = kids(c, rl);
            if vals.is_empty() {
                continue;
            }
            let (mut reachable, mut known) = (false, true);
            for v in vals {
                let (lo, hi) = if c.kind(v) == OpenRange {
                    let k = kids(c, v);
                    if k.len() != 2 {
                        known = false;
                        break;
                    }
                    let bound = |x: NodeId, inf: i128| -> Option<i128> {
                        if c.kind(x) == NameExpr && c.tree.bare_text(x) == "$" {
                            Some(inf)
                        } else {
                            const_value(c, x, 0)
                        }
                    };
                    let (Some(l), Some(h)) = (bound(k[0], i128::MIN / 4), bound(k[1], i128::MAX / 4)) else {
                        known = false;
                        break;
                    };
                    (l.min(h), l.max(h))
                } else {
                    let Some(x) = const_value(c, v, 0) else {
                        known = false;
                        break;
                    };
                    (x, x)
                };
                if hi >= lo_d && lo <= hi_d {
                    reachable = true;
                    break;
                }
            }
            if !known || reachable {
                continue;
            }
            let Some((bname, t)) = bins_name(c, b) else { continue };
            add_n(
                s,
                n,
                "SV-COV-017",
                "UnreachableCoverBin",
                Severity::Warning,
                format!(
                    "Bin '{bname}' of coverpoint '{cp_name}' only lists values outside what the {width}-bit {} coverpoint can hold ({lo_d} to {hi_d}). It can never be hit, so coverage \
                     never reaches 100%. Fix the range or widen the coverpoint expression.",
                    if signed { "signed" } else { "unsigned" }
                ),
                c.loc(t),
            );
        }
    }
}

// ---- assertion helpers ----

/// An immediate assertion's keyword: `assert`, `assume` or `cover`.
fn assert_kw<'a>(c: &'a Cst, a: NodeId) -> &'a str {
    c.toks(a).map(|t| c.text_of(t)).find(|t| matches!(*t, "assert" | "assume" | "cover" | "restrict" | "expect")).unwrap_or("")
}

/// An assertion's pass and fail actions (slang's ifTrue and ifFalse).
fn actions(c: &Cst, a: NodeId) -> (Option<NodeId>, Option<NodeId>) {
    let k = kids(c, a);
    let pass = k.iter().copied().find(|&x| is_stmt(c.kind(x)) && c.kind(x) != EmptyStmt);
    let fail = k.iter().copied().find(|&x| c.kind(x) == ElseClause).and_then(|e| kids(c, e).first().copied());
    (pass, fail)
}

fn has_else(c: &Cst, a: NodeId) -> bool {
    c.tree.child(a, ElseClause).is_some()
}

fn proc_kind<'a>(c: &'a Cst, p: NodeId) -> &'a str {
    c.keyword(p).map_or("", |k| c.text_of(k))
}

// ---- SV-SVA-001 ----

fn immediate_in_comb(c: &Cst, s: &mut Sink) {
    for p in c.nodes(ProceduralBlock) {
        if !matches!(proc_kind(c, p), "always_comb" | "always_latch") {
            continue;
        }
        let n = times(c, p);
        for a in subtree(c, p) {
            if c.kind(a) != AssertStmt {
                continue;
            }
            let toks = tok_texts(c, a);
            if toks.contains(&"final") || toks.contains(&"#") {
                continue;
            }
            if let Some(loc) = start(c, a) {
                add_n(
                    s,
                    n,
                    "SV-SVA-001",
                    "ImmediateAssertInCombinatorialLogic",
                    Severity::Warning,
                    "Simple immediate assertion 'assert (...)' evaluated inside combinatorial process block. \
                     Immediate assertions in active combinatorial logic evaluate transient intermediate states (glitches) \
                     before signal values settle, triggering spurious false-positive failures. Use deferred assertion 'assert #0 (...)' or 'assert final (...)'."
                        .into(),
                    loc,
                );
            }
        }
    }
}

// ---- SV-SVA-002 ----

fn is_ident_name(c: &Cst, e: NodeId) -> bool {
    if c.kind(e) != NameExpr || c.tree.children(e).next().is_some() {
        return false;
    }
    let toks: Vec<u32> = c.toks(e).collect();
    toks.len() == 1 && {
        let t = c.text_of(toks[0]);
        t.starts_with(|ch: char| ch.is_ascii_alphabetic() || ch == '_' || ch == '\\') && !matches!(t, "this" | "super" | "null")
    }
}

/// slang's integer literal or vector expression (`5`, `1'b0`, `'h3`), not
/// an unbased unsized `'0`.
fn is_int_literal(c: &Cst, e: NodeId) -> bool {
    if c.kind(e) != Literal {
        return false;
    }
    let text: String = c.tree.token_range(e).into_iter().map(|t| c.text_of(t)).collect();
    if text.bytes().all(|b| b.is_ascii_digit() || b == b'_') && !text.is_empty() {
        return true;
    }
    match text.find('\'') {
        Some(q) => {
            let rest = text[q + 1..].trim_start_matches(['s', 'S']);
            rest.starts_with(['d', 'D', 'h', 'H', 'b', 'B', 'o', 'O'])
        }
        None => false,
    }
}

fn is_simple_reset(c: &Cst, e: NodeId) -> bool {
    match c.kind(e) {
        NameExpr => is_ident_name(c, e),
        PrefixExpr => {
            let op = c.keyword(e).map(|t| c.text_of(t)).unwrap_or("");
            matches!(op, "!" | "~") && kids(c, e).first().is_some_and(|&x| is_ident_name(c, x))
        }
        ParenExpr => kids(c, e).first().is_some_and(|&x| is_simple_reset(c, x)),
        BinaryExpr => {
            let op = c.keyword(e).map(|t| c.text_of(t)).unwrap_or("");
            let k = kids(c, e);
            matches!(op, "==" | "!=") && k.len() == 2 && is_ident_name(c, k[0]) && is_int_literal(c, k[1])
        }
        _ => false,
    }
}

fn disable_iff_complex(c: &Cst, s: &mut Sink) {
    for d in c.nodes(DisableIff) {
        let Some(pe) = c.parent(d) else { continue };
        if c.kind(pe) != PropertyExpr {
            continue;
        }
        if !c.parent(pe).is_some_and(|p| matches!(c.kind(p), PropertyDecl | ConcurrentAssert)) {
            continue;
        }
        let Some(&e) = kids(c, d).first() else { continue };
        if is_simple_reset(c, e) {
            continue;
        }
        let Some(kw) = c.tok(d, "disable") else { continue };
        let text = slang_text(c, e);
        s.add(
            "SV-SVA-002",
            "DisableIffComplexExpression",
            Severity::Warning,
            format!(
                "'disable iff (...)' clause contains complex expression '{text}'. \
                 In SystemVerilog assertion guidelines (IEEE 1800 § 16.12), \
                 'disable iff' evaluates asynchronously on every signal glitch and should be strictly restricted to \
                 simple reset signals (e.g. '!rst_n') to avoid glitch sensitivity and formal property state explosion."
            ),
            c.loc(kw),
        );
    }
}

// ---- SV-SVA-003 ----

fn edges(c: &Cst, n: NodeId) -> (bool, bool, Option<u32>) {
    let (mut pos, mut neg, mut first) = (false, false, None);
    for e in subtree(c, n) {
        if c.kind(e) != EventExpr {
            continue;
        }
        let Some(t) = c.keyword(e) else { continue };
        match c.text_of(t) {
            "posedge" => pos = true,
            "negedge" => neg = true,
            _ => continue,
        }
        first.get_or_insert(t);
    }
    (pos, neg, first)
}

fn mixed_edges(c: &Cst, s: &mut Sink) {
    let report = |s: &mut Sink, what: String, at: Option<u32>, fallback: Option<u32>| {
        let Some(t) = at.or(fallback) else { return };
        s.add(
            "SV-SVA-003",
            "MixedClockEdgesInProperty",
            Severity::Warning,
            format!(
                "{what} contains mixed clock edge triggers (both 'posedge' and 'negedge'). \
                 In SystemVerilog assertion verification (IEEE 1800 § 16.13), mixing positive and negative clock \
                 edges within the same assertion introduces half-cycle race conditions and causes severe state space \
                 explosion in formal verification (FPV) engines."
            ),
            c.loc(t),
        );
    };
    for p in c.nodes(PropertyDecl) {
        let (pos, neg, first) = edges(c, p);
        if pos && neg {
            let nm = c.tree.child(p, Name).and_then(|x| c.first_tok(x));
            report(s, format!("Property '{}'", decl_name(c, p)), first, nm);
        }
    }
    for a in c.nodes(ConcurrentAssert) {
        let (pos, neg, first) = edges(c, a);
        if pos && neg {
            let kw = assert_kw(c, a);
            report(s, format!("Assertion '{kw} property'"), first, c.tok(a, kw));
        }
    }
}

// ---- SV-SVA-005 ----

fn multi_clock(c: &Cst, s: &mut Sink) {
    for p in c.nodes(PropertyDecl) {
        let mut clocks = BTreeSet::new();
        for e in subtree(c, p) {
            if c.kind(e) != EventExpr {
                continue;
            }
            let Some(&x) = kids(c, e).first() else { continue };
            let t = slang_trimmed(c, x);
            if !t.is_empty() {
                clocks.insert(t);
            }
        }
        if clocks.len() <= 1 {
            continue;
        }
        let list = clocks.iter().map(|x| format!("'{x}'")).collect::<Vec<_>>().join(", ");
        let Some(t) = c.tree.child(p, Name).and_then(|x| c.first_tok(x)) else { continue };
        s.add(
            "SV-SVA-005",
            "MultiClockConcurrentAssertionDomain",
            Severity::Error,
            format!(
                "Property '{}' mixes multiple distinct clock domains ({list}) without explicit clock domain crossing (CDC) synchronization. Under IEEE 1800 § 16.13 and \
                 Under IEEE 1800 § 16.13 assertion rules, multi-clocked assertions without formal CDC qualification introduce simulation \
                 race conditions, non-deterministic sampling, and exponential formal verification state explosion.",
                decl_name(c, p)
            ),
            c.loc(t),
        );
    }
}

// ---- SVA-ACT-001 ----

fn assert_without_else(c: &Cst, s: &mut Sink) {
    for a in ast_asserts(c, AssertStmt) {
        if assert_kw(c, a) != "assert" || has_else(c, a) {
            continue;
        }
        if let Some(loc) = start(c, a) {
            add_n(
                s,
                times(c, a),
                "SVA-ACT-001",
                "AssertWithoutElse",
                Severity::Warning,
                "Immediate assertion lacks an 'else' action block. Assertions without explicit action blocks \
                 (e.g. 'else `uvm_error(\"ASSERT_FAIL\", \"...\")') print generic default messages and bypass UVM report catchers."
                    .into(),
                loc,
            );
        }
    }
    for a in ast_asserts(c, ConcurrentAssert) {
        if assert_kw(c, a) != "assert" || has_else(c, a) {
            continue;
        }
        if let Some(loc) = start(c, a) {
            add_n(
                s,
                times(c, a),
                "SVA-ACT-001",
                "AssertWithoutElse",
                Severity::Warning,
                "Concurrent assertion lacks an 'else' action block. When this assertion fails, the simulator emits \
                 a generic one-line diagnostic without severity filtering or payload context. Add 'else `uvm_error(...)'."
                    .into(),
                loc,
            );
        }
    }
}

// ---- SVA-ACT-002 ----

fn time_in_action(c: &Cst, s: &mut Sink) {
    for (kind, label) in [(AssertStmt, "immediate"), (ConcurrentAssert, "concurrent")] {
        for a in ast_asserts(c, kind) {
            let n = times(c, a);
            let (pass, fail) = actions(c, a);
            for act in [pass, fail].into_iter().flatten() {
                let mut last = None;
                for x in subtree(c, act) {
                    let timed = match c.kind(x) {
                        TimedStmt => true,
                        WaitStmt => c.keyword(x).is_some_and(|k| c.text_of(k) == "wait") && !c.toks(x).any(|t| c.text_of(t) == "fork"),
                        _ => false,
                    };
                    if timed {
                        last = start(c, x);
                    }
                }
                let Some(loc) = last else { continue };
                add_n(
                    s,
                    n,
                    "SVA-ACT-002",
                    "TimeInAssertionAction",
                    Severity::Error,
                    format!(
                        "Time-consuming statement (#delay, @event, wait) detected inside {label} assertion action block. In SystemVerilog (IEEE 1800 § 16), assertion action blocks execute in the Reactive region and must not consume simulation time."
                    ),
                    loc,
                );
            }
        }
    }
}

// ---- SVA-ACT-003 ----

/// How often `swe::UserCodeVisitor` reaches a node: never in library code,
/// once per module instance, and once for a parameterized class (its
/// default specialization only).
pub(crate) fn user_code_times(c: &Cst, n: NodeId) -> usize {
    if is_library(c, n) {
        return 0;
    }
    let generic = ancestors(c, n).filter(|&a| c.kind(a) == ClassDecl).count();
    if generic == 0 {
        return times(c, n);
    }
    let unit = ancestors(c, n).find(|&a| matches!(c.kind(a), ModuleDecl | InterfaceDecl | ProgramDecl));
    match unit {
        Some(u) => times(c, u),
        None => 1,
    }
}

fn side_effect_in_action(c: &Cst, s: &mut Sink) {
    for kind in [AssertStmt, ConcurrentAssert] {
        for a in ast_asserts(c, kind) {
            let n = user_code_times(c, a);
            if n == 0 {
                continue;
            }
            let (pass, fail) = actions(c, a);
            for act in [pass, fail].into_iter().flatten() {
                let inside: HashSet<NodeId> = subtree(c, act).into_iter().collect();
                let mut names: Vec<String> = written(c, act, true)
                    .into_iter()
                    .filter(|w| {
                        // A variable declared in a block of the action itself.
                        !w.2.is_some_and(|d| {
                            d.kind == crate::g_c_support::DeclKind::Local
                                && inside.contains(&d.node)
                                && c.parent(d.node).and_then(|p| c.parent(p)).is_some_and(|b| matches!(c.kind(b), SeqBlock | ParBlock))
                        })
                    })
                    .map(|w| w.1)
                    .collect();
                if names.is_empty() {
                    continue;
                }
                names.sort();
                let list = names.iter().map(|x| format!("'{x}'")).collect::<Vec<_>>().join(", ");
                let Some(loc) = start(c, act) else { continue };
                add_n(
                    s,
                    n,
                    "SVA-ACT-003",
                    "SideEffectInAssertionAction",
                    Severity::Warning,
                    format!(
                        "Assertion action block changes {list}. When assertions are turned off the action never runs, so the testbench behaves \
                         differently. Keep action blocks to reporting, and update state outside the assertion."
                    ),
                    loc,
                );
            }
        }
    }
}

// ---- SVA-CTX-001 ----

fn concurrent_in_function(c: &Cst, s: &mut Sink) {
    for a in c.nodes(ConcurrentAssert) {
        let Some(r) = ancestors(c, a).find(|&x| matches!(c.kind(x), FunctionDecl | TaskDecl)) else { continue };
        let name: String = decl_name(c, r).chars().filter(|ch| !ch.is_whitespace()).collect();
        let Some(loc) = start(c, a) else { continue };
        s.add(
            "SVA-CTX-001",
            "ConcurrentAssertInFunction",
            Severity::Error,
            format!(
                "Illegal concurrent assertion inside function '{name}'. Functions in SystemVerilog (IEEE 1800 § 13.4.4 & § 16.14.1) execute in zero simulation time and cannot participate in clock-domain sampled value checking. Move the assertion to a module, interface, checker, or task."
            ),
            loc,
        );
    }
}

/// Assertion statements that slang's AST keeps. A concurrent assertion inside
/// a function is illegal, and an immediate assertion whose condition fails to
/// bind becomes an invalid statement; AST rules never see either. Only one
/// binding failure is modelled: a scoped name (`p::f`) whose head is not a
/// known class, package or typedef (e.g. `uvm_resource_db#(T)::read_by_name`
/// without UVM on the include path).
fn ast_asserts(c: &Cst, kind: SyntaxKind) -> Vec<NodeId> {
    let known = known_scopes(c);
    c.nodes(kind)
        .into_iter()
        .filter(|&a| match kind {
            ConcurrentAssert => !ancestors(c, a).any(|x| c.kind(x) == FunctionDecl),
            _ => !unknown_scope_in_condition(c, a, &known),
        })
        .collect()
}

fn known_scopes(c: &Cst) -> HashSet<String> {
    let mut out: HashSet<String> = ["std", "$unit", "local", "super", "this"].iter().map(|s| s.to_string()).collect();
    for k in [ClassDecl, PackageDecl, TypedefDecl, ForwardTypedef, InterfaceDecl] {
        for n in c.nodes(k) {
            out.insert(last_name(c, n));
        }
    }
    out
}

fn unknown_scope_in_condition(c: &Cst, a: NodeId, known: &HashSet<String>) -> bool {
    kids(c, a)
        .into_iter()
        .filter(|&x| !is_stmt(c.kind(x)) && c.kind(x) != ElseClause)
        .flat_map(|x| subtree(c, x))
        .filter(|&x| c.kind(x) == ScopedExpr)
        .filter_map(|x| kids(c, x).first().copied())
        .filter(|&h| c.kind(h) == NameExpr)
        .any(|h| {
            let head = c.toks(h).next().map(|t| c.text_of(t).to_string()).unwrap_or_default();
            !head.is_empty() && !known.contains(&head)
        })
}
