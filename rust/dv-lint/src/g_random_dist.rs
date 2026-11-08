//! Group E (builder slice N6.3): the dist and solve-before checks of
//! `cpp/src/rules/random/RandomizationGotchas.cpp`. See [`crate::g_random`].

use std::collections::{HashMap, HashSet};

use sv_syntax::NodeId;

use crate::g_random::{declarators, is_parameterized, qualifiers, expression_constraint, kids, model_classes, subtree, user_code};
use crate::g_random_bug::base_name;
use crate::g_random_oop::{strip_conv, unparen};
use crate::g_random_sema::ast_ok;
use crate::syntax::Cst;
use crate::Check::Syntax;
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-RND-025", name: "DistWideRangeColonEq", check: Syntax(dist_wide_range) },
        Rule { id: "SV-RND-006", name: "RandcMemoryExplosion", check: Syntax(randc_wide) },
        Rule { id: "SV-RND-017", name: "RandcStateExhaustion", check: Syntax(randc_narrow) },
        Rule { id: "SV-RND-019", name: "ConstraintImplicationMultiBit", check: Syntax(implication_multi_bit) },
        Rule { id: "SV-RND-023", name: "UnallocatedRandObjectArray", check: Syntax(unallocated_rand_array) },
        Rule { id: "SV-RND-026", name: "UnrelatedSolveBefore", check: Syntax(unrelated_solve_before) },
    ]
}

/// Classes the UVM model lists, without the library's: (class, name).
fn user_model_classes(c: &Cst) -> Vec<(NodeId, String)> {
    model_classes(c).into_iter().filter(|(k, n)| user_code(c, *k) && !n.starts_with("uvm_")).collect()
}

/// The class's constraint blocks that bound without error.
fn constraint_blocks(c: &Cst, cls: NodeId) -> Vec<NodeId> {
    c.tree
        .children(cls)
        .filter(|&d| c.kind(d) == ConstraintDecl && ast_ok(c, d))
        .filter_map(|d| c.tree.child(d, ConstraintBlock))
        .collect()
}

// ---- SV-RND-025 ----

/// The value of an integer literal (`8'hFF`, `1_000`, `'d5`).
fn literal_value(text: &str) -> Option<i64> {
    let t: String = text.chars().filter(|&x| x != '_' && !x.is_whitespace()).collect();
    let (digits, radix) = match t.find('\'') {
        Some(q) => {
            let rest = t[q + 1..].trim_start_matches(['s', 'S']);
            let mut ch = rest.chars();
            let radix = match ch.next()?.to_ascii_lowercase() {
                'h' => 16,
                'd' => 10,
                'b' => 2,
                'o' => 8,
                _ => return None,
            };
            (ch.as_str().to_string(), radix)
        }
        None => (t, 10),
    };
    i64::from_str_radix(&digits, radix).ok()
}

/// A constant integer: literals, parameters and arithmetic over them.
fn const_int(c: &Cst, e: NodeId, params: &HashMap<String, NodeId>, depth: u32) -> Option<i64> {
    if depth > 16 {
        return None;
    }
    let e = unparen(c, strip_conv(c, e).0);
    match c.kind(e) {
        Literal => literal_value(&c.tree.bare_text(e)),
        NameExpr => const_int(c, *params.get(&c.tree.bare_text(e))?, params, depth + 1),
        PrefixExpr => {
            let v = const_int(c, *kids(c, e).first()?, params, depth + 1)?;
            match c.text_of(c.first_tok(e)?) {
                "-" => Some(-v),
                "+" => Some(v),
                _ => None,
            }
        }
        BinaryExpr => {
            let k = kids(c, e);
            let (a, b) = (const_int(c, *k.first()?, params, depth + 1)?, const_int(c, *k.get(1)?, params, depth + 1)?);
            match crate::g_random::op(c, e) {
                "+" => a.checked_add(b),
                "-" => a.checked_sub(b),
                "*" => a.checked_mul(b),
                "/" if b != 0 => Some(a / b),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Parameters with a value: name -> value expression.
fn param_values(c: &Cst) -> HashMap<String, NodeId> {
    let mut out = HashMap::new();
    for d in c.nodes(ParamDecl) {
        for (dcl, v, _) in declarators(c, d) {
            if let Some(&e) = kids(c, dcl).iter().rev().find(|&&k| crate::g_random_sema::is_expr(c.kind(k))) {
                out.insert(v, e);
            }
        }
    }
    out
}

/// Number of values `[lo:hi]` covers.
fn range_width(c: &Cst, value: NodeId, params: &HashMap<String, NodeId>) -> Option<i64> {
    if c.kind(value) != OpenRange || c.tok(value, ":").is_none() {
        return None;
    }
    let k = kids(c, value);
    let [lo, hi] = k.as_slice() else { return None };
    let (lo, hi) = (const_int(c, *lo, params, 0)?, const_int(c, *hi, params, 0)?);
    Some((hi - lo).abs() + 1)
}

fn dist_wide_range(c: &Cst, s: &mut Sink) {
    let params = param_values(c);
    for (cls, _) in user_model_classes(c) {
        for block in constraint_blocks(c, cls) {
            for item in subtree(c, block).into_iter().filter(|&x| c.kind(x) == ConstraintItem) {
                let Some((_, e)) = expression_constraint(c, item) else { continue };
                let e = unparen(c, e);
                if c.kind(e) != ExprOrDist || c.tok(e, "dist").is_none() {
                    continue;
                }
                let Some(list) = c.tree.child(e, RangeList) else { continue };
                let items: Vec<NodeId> = c.tree.children(list).filter(|&x| c.kind(x) == DistItem).collect();
                let widths: Vec<Option<i64>> = items
                    .iter()
                    .map(|&it| c.tree.children(it).next().and_then(|v| range_width(c, v, &params)))
                    .collect();
                if widths.iter().all(|w| w.is_some() && *w == widths[0]) {
                    continue;
                }
                for (&it, w) in items.iter().zip(&widths) {
                    let Some(w) = *w else { continue };
                    if w <= 16 || c.tok(it, ":=").is_none() {
                        continue;
                    }
                    let Some(at) = c.tree.children(it).next().and_then(|v| c.first_tok(v)) else { continue };
                    s.add(
                        "SV-RND-025",
                        "DistWideRangeColonEq",
                        Severity::Warning,
                        format!(
                            "':=' gives every one of the {w} values in this range the full weight, so the range \
                             outweighs the single values next to it. Use ':/' to share the weight across the range, \
                             or write the per-value weight on purpose."
                        ),
                        c.loc(at),
                    );
                }
            }
        }
    }
}

// ---- SV-RND-026 ----

/// Union-find over variable names.
#[derive(Default)]
struct Relations {
    parent: HashMap<String, String>,
}

impl Relations {
    fn find(&mut self, s: &str) -> String {
        let p = self.parent.entry(s.to_string()).or_insert_with(|| s.to_string()).clone();
        if p == s {
            return p;
        }
        let r = self.find(&p);
        self.parent.insert(s.to_string(), r.clone());
        r
    }

    fn relate(&mut self, group: &HashSet<String>) {
        let mut first: Option<String> = None;
        for g in group {
            match &first {
                None => {
                    self.find(g);
                    first = Some(g.clone());
                }
                Some(f) => {
                    let (a, b) = (self.find(g), self.find(f));
                    self.parent.insert(a, b);
                }
            }
        }
    }

    fn related(&mut self, a: &str, b: &str) -> bool {
        self.parent.contains_key(a) && self.parent.contains_key(b) && self.find(a) == self.find(b)
    }
}

/// Every name a constraint item reads (swe::referencedSymbols): the names
/// of its expressions and the members they access, guards included.
fn item_names(c: &Cst, item: NodeId) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut loop_vars = HashSet::new();
    for x in subtree(c, item) {
        match c.kind(x) {
            NameExpr => {
                out.insert(c.tree.bare_text(x));
            }
            MemberExpr => {
                let base = c.tree.children(x).next().map(|b| c.tree.bare_text(b));
                let m = c.last_text(x).to_string();
                // `this.x` names x itself.
                if base.as_deref() == Some("this") {
                    out.insert(m);
                } else {
                    out.insert(format!(".{m}"));
                }
            }
            ConstraintItem if c.toks(x).next().is_some_and(|t| c.text_of(t) == "foreach") => {
                // The array a foreach walks; its loop variables are symbols
                // of their own, which relate nothing outside the item.
                let toks: Vec<u32> = c.toks(x).collect();
                let mut depth = 0;
                for &t in &toks {
                    match c.text_of(t) {
                        "[" => depth += 1,
                        "]" => depth -= 1,
                        v if depth == 1 && c.tree.tokens[t as usize].kind == sv_syntax::TokenKind::Identifier => {
                            loop_vars.insert(v.to_string());
                        }
                        _ => {}
                    }
                }
                if let Some(&t) = toks.get(2) {
                    let name = c.text_of(t);
                    if name == "this" {
                        if let Some(&m) = toks.get(4) {
                            out.insert(c.text_of(m).to_string());
                        }
                    } else {
                        out.insert(name.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    out.retain(|n| !loop_vars.contains(n));
    out
}

/// Entries of a solve-before list: (first token, root name).
type SolveList = Vec<(u32, String)>;

/// A solve-before item's two lists.
fn solve_lists(c: &Cst, item: NodeId) -> Option<(SolveList, SolveList)> {
    let toks: Vec<u32> = c.toks(item).collect();
    if c.text_of(*toks.first()?) != "solve" {
        return None;
    }
    let mut lists = [Vec::new(), Vec::new()];
    let (mut which, mut depth) = (0, 0i32);
    let mut entry: Vec<u32> = Vec::new();
    let flush = |lists: &mut [SolveList; 2], which: usize, entry: &mut Vec<u32>| {
        // swe::rootSymbol: through selects and member accesses, not calls.
        let mut d = 0;
        let call = entry.iter().any(|&t| {
            match c.text_of(t) {
                "[" => d += 1,
                "]" => d -= 1,
                "(" if d == 0 => return true,
                _ => {}
            }
            false
        });
        if let (Some(&first), false) = (entry.first(), call) {
            lists[which].push((first, c.text_of(first).to_string()));
        }
        entry.clear();
    };
    for &t in &toks[1..] {
        let x = c.text_of(t);
        match x {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => depth -= 1,
            _ => {}
        }
        if depth == 0 && matches!(x, "before" | "," | ";") {
            flush(&mut lists, which, &mut entry);
            if x == "before" {
                which = 1;
            }
        } else {
            entry.push(t);
        }
    }
    let [a, b] = lists;
    Some((a, b))
}

fn relate_block(c: &Cst, block: NodeId, rel: &mut Relations) {
    for item in c.tree.children(block).filter(|&x| c.kind(x) == ConstraintItem) {
        if c.toks(item).next().is_some_and(|t| c.text_of(t) == "solve") {
            continue;
        }
        rel.relate(&item_names(c, item));
    }
}

fn unrelated_solve_before(c: &Cst, s: &mut Sink) {
    let classes = user_model_classes(c);
    let by_name: HashMap<String, NodeId> = classes.iter().map(|(k, n)| (n.clone(), *k)).collect();
    for (cls, _) in &classes {
        let mut rel = Relations::default();
        let mut cur = Some(*cls);
        let mut guard = 0;
        while let Some(k) = cur {
            for b in constraint_blocks(c, k) {
                relate_block(c, b, &mut rel);
            }
            guard += 1;
            cur = if guard < 64 { base_name(c, k).and_then(|b| by_name.get(&b).copied()) } else { None };
        }
        for block in constraint_blocks(c, *cls) {
            for item in subtree(c, block).into_iter().filter(|&x| c.kind(x) == ConstraintItem) {
                let Some((solve, after)) = solve_lists(c, item) else { continue };
                for (at, a) in &solve {
                    for (_, b) in &after {
                        if a == b || rel.related(a, b) {
                            continue;
                        }
                        s.add(
                            "SV-RND-026",
                            "UnrelatedSolveBefore",
                            Severity::Warning,
                            format!(
                                "'solve {a} before {b}': no constraint relates these variables, so the order changes \
                                 nothing. Remove it, or add the constraint it was meant to shape."
                            ),
                            c.loc(*at),
                        );
                    }
                }
            }
        }
    }
}

// ---- SV-RND-006 / SV-RND-017 ----

/// Width of one dimension, `None` for a dynamic one.
fn dim_width(c: &Cst, d: NodeId, params: &HashMap<String, NodeId>) -> Option<i64> {
    let k = kids(c, d);
    match (k.as_slice(), c.tok(d, ":").is_some()) {
        ([lo, hi], true) => Some((const_int(c, *hi, params, 0)? - const_int(c, *lo, params, 0)?).abs() + 1),
        ([n], false) if c.tok(d, "$").is_none() => const_int(c, *n, params, 0),
        _ => None,
    }
}

/// Bit width of a data type (slang's getBitWidth), when the syntax gives it.
fn type_width(
    c: &Cst,
    dt: NodeId,
    params: &HashMap<String, NodeId>,
    typedefs: &HashMap<String, NodeId>,
    depth: u32,
) -> Option<i64> {
    if depth > 8 {
        return None;
    }
    let ft = c.first_tok(dt)?;
    let first = c.text_of(ft);
    let base = match first {
        "bit" | "logic" | "reg" => 1,
        "byte" => 8,
        "shortint" => 16,
        "int" | "integer" => 32,
        "longint" | "time" => 64,
        "enum" => match c.tree.child(dt, DataType) {
            Some(b) => type_width(c, b, params, typedefs, depth + 1)?,
            None => 32,
        },
        _ if c.tree.tokens[ft as usize].kind == sv_syntax::TokenKind::Identifier
            && c.tree.token_range(dt).len() == 1 =>
        {
            type_width(c, *typedefs.get(first)?, params, typedefs, depth + 1)?
        }
        _ => return None,
    };
    let mut w = base;
    for d in c.tree.children(dt).filter(|&x| c.kind(x) == Dim) {
        w *= dim_width(c, d, params)?;
    }
    Some(w)
}

/// Each randc variable with its width: (name token, name, width).
fn randc_vars(c: &Cst) -> Vec<(u32, String, i64)> {
    let params = param_values(c);
    let typedefs = typedefs(c);
    let mut out = Vec::new();
    for d in c.nodes(DataDecl) {
        if !qualifiers(c, d).contains(&"randc") {
            continue;
        }
        if c.parent(d).is_some_and(|p| c.kind(p) == ClassDecl && is_parameterized(c, p)) {
            continue;
        }
        let Some(dt) = c.tree.child(d, DataType) else { continue };
        let Some(w) = type_width(c, dt, &params, &typedefs, 0) else { continue };
        // An unpacked array has no bit width in slang (getBitWidth is 0).
        for (dcl, name, tok) in declarators(c, d) {
            if !c.tree.children(dcl).any(|x| c.kind(x) == Dim) {
                out.push((tok, name, w));
            }
        }
    }
    out
}

fn randc_wide(c: &Cst, s: &mut Sink) {
    for (tok, name, w) in randc_vars(c) {
        if w <= 16 {
            continue;
        }
        s.add(
            "SV-RND-006",
            "RandcMemoryExplosion",
            Severity::Error,
            format!(
                "Cyclic random variable '{name}' is declared as 'randc' with a width of {w} bits (2^{w} states). In IEEE \
                 1800 § 18.4.1, randc variables require the constraint solver to maintain a non-repeating permutation \
                 table across the entire state space. Vectors wider than 16 bits cause massive simulator heap exhaustion \
                 and extreme runtime deadlocks during solver initialization. Restrict 'randc' to variables <= 16 bits, or \
                 use 'rand'."
            ),
            c.loc(tok),
        );
    }
}

fn randc_narrow(c: &Cst, s: &mut Sink) {
    for (tok, name, w) in randc_vars(c) {
        if !(1..=2).contains(&w) {
            continue;
        }
        let states = 1u64 << w;
        let plural = if w > 1 { "s" } else { "" };
        s.add(
            "SV-RND-017",
            "RandcStateExhaustion",
            Severity::Warning,
            format!(
                "Cyclic random variable '{name}' is declared as 'randc' with a width of {w} bit{plural} (only {states} \
                 states). In IEEE 1800 § 18.4.1, 'randc' forces the constraint solver to maintain a non-repeating \
                 permutation table. For variables <= 2 bits, cyclic permutation history resets every {states} iterations, \
                 incurring solver tracking overhead with no statistical permutation benefit. Use standard 'rand' instead."
            ),
            c.loc(tok),
        );
    }
}

// ---- SV-RND-019 ----

/// Typedefs by name (no unpacked dimensions).
fn typedefs(c: &Cst) -> HashMap<String, NodeId> {
    let mut out = HashMap::new();
    for t in c.nodes(TypedefDecl) {
        if let (Some(dt), Some(n)) = (c.tree.child(t, DataType), c.tree.child(t, Name)) {
            if !c.tree.children(t).any(|x| c.kind(x) == Dim) {
                out.insert(c.tree.bare_text(n), dt);
            }
        }
    }
    out
}

struct Widths<'a> {
    params: HashMap<String, NodeId>,
    typedefs: HashMap<String, NodeId>,
    classes: &'a HashMap<String, NodeId>,
}

impl Widths<'_> {
    /// Width of a scalar property of `cls` or its bases.
    fn prop(&self, c: &Cst, cls: NodeId, name: &str) -> Option<i64> {
        let mut cur = Some(cls);
        let mut guard = 0;
        while let Some(k) = cur {
            for d in c.tree.children(k).filter(|&d| c.kind(d) == DataDecl) {
                if qualifiers(c, d).contains(&"typedef") {
                    continue;
                }
                for (dcl, v, _) in declarators(c, d) {
                    if v == name {
                        if c.tree.children(dcl).any(|x| c.kind(x) == Dim) {
                            return None;
                        }
                        return type_width(c, c.tree.child(d, DataType)?, &self.params, &self.typedefs, 0);
                    }
                }
            }
            guard += 1;
            cur = if guard < 64 { base_name(c, k).and_then(|b| self.classes.get(&b).copied()) } else { None };
        }
        None
    }

    /// Self-determined width of an integral expression, when the syntax
    /// gives it.
    fn expr(&self, c: &Cst, cls: NodeId, e: NodeId) -> Option<i64> {
        let k = kids(c, e);
        match c.kind(e) {
            ParenExpr => self.expr(c, cls, *k.first()?),
            NameExpr => self.prop(c, cls, &c.tree.bare_text(e)),
            Literal => {
                let t = c.tree.bare_text(e);
                match t.find('\'') {
                    _ if t.starts_with('"') => None,
                    Some(0) if t.len() == 2 && !t[1..].starts_with(['h', 'd', 'b', 'o', 'H', 'D', 'B', 'O']) => Some(1),
                    Some(0) => Some(32),
                    Some(q) => t[..q].parse().ok(),
                    None if t.chars().all(|x| x.is_ascii_digit() || x == '_') => Some(32),
                    None => None,
                }
            }
            PrefixExpr => match c.text_of(c.first_tok(e)?) {
                "!" | "&" | "|" | "^" | "~&" | "~|" | "~^" | "^~" => Some(1),
                "~" | "-" | "+" => self.expr(c, cls, *k.first()?),
                _ => None,
            },
            BinaryExpr => match crate::g_random::op(c, e) {
                "==" | "!=" | "===" | "!==" | "==?" | "!=?" | "<" | "<=" | ">" | ">=" | "&&" | "||" | "->" | "<->" => {
                    Some(1)
                }
                "+" | "-" | "*" | "/" | "%" | "&" | "|" | "^" | "~^" | "^~" => {
                    Some(self.expr(c, cls, *k.first()?)?.max(self.expr(c, cls, *k.get(1)?)?))
                }
                "<<" | ">>" | "<<<" | ">>>" | "**" => self.expr(c, cls, *k.first()?),
                _ => None,
            },
            SelectExpr => {
                let w = self.expr(c, cls, *k.first()?)?;
                match (k.len(), c.tok(e, ":").is_some(), c.tok(e, "+:").or(c.tok(e, "-:")).is_some()) {
                    (2, false, false) if w > 1 => Some(1),
                    (3, true, false) => {
                        let (a, b) = (const_int(c, k[1], &self.params, 0)?, const_int(c, k[2], &self.params, 0)?);
                        Some((a - b).abs() + 1)
                    }
                    (3, false, true) => const_int(c, k[2], &self.params, 0),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

fn implication_multi_bit(c: &Cst, s: &mut Sink) {
    let classes: HashMap<String, NodeId> = model_classes(c).into_iter().map(|(k, n)| (n, k)).collect();
    let w = Widths { params: param_values(c), typedefs: typedefs(c), classes: &classes };
    for (cls, _) in model_classes(c) {
        for block in constraint_blocks(c, cls) {
            for item in subtree(c, block).into_iter().filter(|&x| c.kind(x) == ConstraintItem) {
                if c.tok(item, "->").is_none() {
                    continue;
                }
                let Some(pred) = c.tree.children(item).next() else { continue };
                let Some(width) = w.expr(c, cls, pred).filter(|&x| x > 1) else { continue };
                let Some(at) = c.first_tok(unparen(c, pred)) else { continue };
                s.add(
                    "SV-RND-019",
                    "ConstraintImplicationMultiBit",
                    Severity::Warning,
                    format!(
                        "Constraint implication operator '->' has a multi-bit condition expression of width {width} bits \
                         without an explicit boolean comparison. In SystemVerilog constraint solving (IEEE 1800 § 18.5.6), \
                         multi-bit implication is often confused with bitwise masking; use an explicit comparison (e.g. \
                         'expr != 0 -> ...') to make constraint intent unambiguous."
                    ),
                    c.loc(at),
                );
            }
        }
    }
}

// ---- SV-RND-023 ----

fn unallocated_rand_array(c: &Cst, s: &mut Sink) {
    let classes: HashMap<String, NodeId> = class_names(c);
    // Arrays an element of which is assigned or pushed anywhere, by name.
    let mut populated: HashSet<String> = HashSet::new();
    let last_name = |x: NodeId| match c.kind(x) {
        NameExpr | MemberExpr => Some(c.last_text(x).to_string()),
        _ => None,
    };
    for n in c.nodes(BinaryExpr) {
        if !matches!(crate::g_random::op(c, n), "=" | "<=") {
            continue;
        }
        let Some(l) = c.tree.children(n).next().filter(|&l| c.kind(l) == SelectExpr) else { continue };
        if let Some(v) = c.tree.children(l).next().and_then(last_name) {
            populated.insert(v);
        }
    }
    for call in c.nodes(CallExpr) {
        if !matches!(crate::g_random::call_name(c, call), Some("push_back" | "push_front" | "insert")) {
            continue;
        }
        let Some(callee) = c.tree.children(call).next().filter(|&x| c.kind(x) == MemberExpr) else { continue };
        if let Some(v) = c.tree.children(callee).next().and_then(last_name) {
            populated.insert(v);
        }
    }
    for d in c.nodes(DataDecl) {
        let q = qualifiers(c, d);
        if !q.contains(&"rand") && !q.contains(&"randc") {
            continue;
        }
        let Some(cls) = c.parent(d).filter(|&p| c.kind(p) == ClassDecl) else { continue };
        if is_parameterized(c, cls) {
            continue;
        }
        let Some(dt) = c.tree.child(d, DataType) else { continue };
        if !crate::g_random_bug::is_class_type(c, Some(dt), &classes) {
            continue;
        }
        for (dcl, name, tok) in declarators(c, d) {
            if !c.tree.children(dcl).any(|x| c.kind(x) == Dim) || populated.contains(&name) {
                continue;
            }
            s.add(
                "SV-RND-023",
                "UnallocatedRandObjectArray",
                Severity::Error,
                format!(
                    "rand array '{name}' holds class handles, but no element is ever created (no '{name}[i] = new' or \
                     type_id::create). randomize() leaves the elements null, so their contents are never randomized. \
                     Create the elements, for example in pre_randomize() after sizing the array."
                ),
                c.loc(tok),
            );
        }
    }
}

/// Class declarations by name (out-of-block names left out).
pub(crate) fn class_names(c: &Cst) -> HashMap<String, NodeId> {
    crate::g_random::class_decls(c)
        .into_iter()
        .filter(|&k| !c.tree.child(k, Header).is_some_and(|h| c.tok(h, "::").is_some()))
        .filter_map(|k| Some((crate::g_random::decl_name(c, k)?.0, k)))
        .collect()
}
