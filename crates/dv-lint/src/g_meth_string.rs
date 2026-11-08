//! Group A rules on values used as codes: strings compared against
//! literals to choose what to do (SV-CNS-004 for variables, SV-CPL-001 for
//! routine arguments) and vectors cut into fixed part-selects (SV-CNS-005).

use std::collections::{BTreeSet, HashMap, HashSet};

use dv_model::{Class, ClassId, Design, Loc};
use sv_syntax::NodeId;

use crate::g_meth_body::{bodies, case_parts, if_parts, Calls};
use crate::g_meth_body2::free_routines;
use crate::g_meth_model::overrides_base;
use crate::g_methodology::{class_of_type_in, is_library, Ctx, Index, Typedefs};
use crate::syntax::Cst;
use crate::Check::Mixed;
use crate::{Rule, Severity, Sink};
use sv_syntax::SyntaxKind::*;

pub(crate) fn rules() -> Vec<Rule> {
    vec![
        Rule { id: "SV-CNS-004", name: "StringAsEnum", check: Mixed(string_as_enum) },
        Rule { id: "SV-CNS-005", name: "ManualBitFieldSlicing", check: Mixed(manual_bit_field_slicing) },
        Rule { id: "SV-CPL-001", name: "StringModeParameter", check: Mixed(string_mode_parameter) },
    ]
}

// ---- symbols of a routine ----

/// A variable a name refers to: a routine's local or argument, or a class
/// property (by the class that declares it).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum Sym {
    Local(usize, String),
    Prop(ClassId, String),
}

#[derive(Clone)]
struct SymInfo {
    sym: Sym,
    name: String,
    ty: String,
    formal: bool,
    library: bool,
    loc: Loc,
}

/// One user routine: its tree, declaration, body and (for methods) class
/// and model method.
struct Routine<'a, 'd, 't> {
    id: usize,
    c: &'a Cst<'t>,
    decl: NodeId,
    body: NodeId,
    cls: Option<&'d Class>,
    m: Option<&'d dv_model::Method>,
    /// Names it declares: name -> (type, is an argument, where).
    names: HashMap<String, (String, bool, Loc)>,
}

fn routine_names(c: &Cst, r: NodeId) -> HashMap<String, (String, bool, Loc)> {
    let mut out = HashMap::new();
    for n in c.subtree(r) {
        if !matches!(c.kind(n), Port | DataDecl) {
            continue;
        }
        let ty = c.tree.child(n, DataType).map(|t| c.tree.bare_text(t)).unwrap_or_default();
        for dcl in c.tree.children(n).filter(|&x| c.kind(x) == Declarator) {
            let Some(name) = c.tree.child(dcl, Name) else { continue };
            let unpacked = c.tree.child(dcl, Dim).is_some();
            let t = if unpacked { format!("{ty}$[]") } else { ty.clone() };
            out.entry(c.tree.bare_text(name)).or_insert((t, c.kind(n) == Port, c.loc_of(name)));
        }
    }
    out
}

fn user_routines<'a, 'd, 't>(d: &'d Design, ix: &Index<'a, 't>) -> Vec<Routine<'a, 'd, 't>> {
    let mut out = Vec::new();
    for (cls, m, c, body) in bodies(d, ix) {
        let Some(decl) = c.parent(body) else { continue };
        out.push(Routine { id: out.len(), c, decl, body, cls: Some(cls), m: Some(m), names: routine_names(c, decl) });
    }
    for c in ix.csts {
        for decl in free_routines(d, c) {
            // Out-of-block method bodies are the class's.
            if crate::g_meth_gotcha::in_library(d, c, decl)
                || c.tree.child(decl, Name).is_some_and(|n| c.tree.bare_text(n).contains("::"))
            {
                continue;
            }
            let Some(body) = c.tree.child(decl, Body) else { continue };
            out.push(Routine { id: out.len(), c, decl, body, cls: None, m: None, names: routine_names(c, decl) });
        }
    }
    out
}

/// Property `name` of `k` or a base, with its declaring class.
fn find_prop<'d>(d: &'d Design, k: &'d Class, name: &str) -> Option<(&'d Class, &'d dv_model::Property)> {
    std::iter::once(k)
        .chain(d.ancestors(k.id))
        .find_map(|x| x.properties.iter().find(|p| p.name == name).map(|p| (x, p)))
}

fn typedef_base(td: &Typedefs, ty: &str) -> String {
    let mut t = ty.replace(' ', "");
    for _ in 0..16 {
        let head = t.split(['[', '$']).next().unwrap_or("").to_string();
        let rest = t[head.len()..].to_string();
        match td.get(&head) {
            Some(b) if *b != head => t = format!("{}{rest}", b.replace(' ', "")),
            _ => break,
        }
    }
    t
}

/// The variable an expression names (slang's referencedSymbol of a named
/// value or member access).
fn referenced(d: &Design, ix: &Index, r: &Routine, e: NodeId) -> Option<SymInfo> {
    let c = r.c;
    let e = c.strip(e);
    match c.kind(e) {
        NameExpr if c.tree.children(e).next().is_none() => {
            let name = c.tree.bare_text(e);
            if let Some((ty, formal, loc)) = r.names.get(&name) {
                return Some(SymInfo {
                    sym: Sym::Local(r.id, name.clone()),
                    name,
                    ty: ty.clone(),
                    formal: *formal,
                    library: false,
                    loc: *loc,
                });
            }
            let (k, p) = find_prop(d, r.cls?, &name)?;
            Some(SymInfo { sym: Sym::Prop(k.id, name.clone()), name, ty: p.ty.clone(), formal: false, library: is_library(d, k), loc: p.loc })
        }
        MemberExpr => {
            let obj = c.tree.children(e).next()?;
            let member = c.last_text(e).to_string();
            let o = c.strip(obj);
            let oc = if c.kind(o) == NameExpr && matches!(c.tree.bare_text(o).as_str(), "this" | "super") {
                let k = r.cls?;
                if c.tree.bare_text(o) == "super" { k.base.map(|b| d.class(b))? } else { k }
            } else if let Some(cls) = r.cls {
                let ctx = Ctx { d, cls, m: r.m, td: Some(&ix.typedefs) };
                ctx.class_of(c, o)?
            } else {
                // A free routine: a local or argument of class type.
                if c.kind(o) != NameExpr {
                    return None;
                }
                let (ty, ..) = r.names.get(&c.tree.bare_text(o))?;
                class_of_type_in(d, Some(&ix.typedefs), ty, None)?
            };
            let (k, p) = find_prop(d, oc, &member)?;
            Some(SymInfo {
                sym: Sym::Prop(k.id, member.clone()),
                name: member,
                ty: p.ty.clone(),
                formal: false,
                library: is_library(d, k),
                loc: p.loc,
            })
        }
        _ => None,
    }
}

fn is_string_type(td: &Typedefs, ty: &str) -> bool {
    typedef_base(td, ty) == "string"
}

/// An integral type that is not a struct or packed union.
fn is_integral_type(td: &Typedefs, ty: &str) -> bool {
    let t = typedef_base(td, ty);
    if t.contains('$') {
        return false;
    }
    const BASES: &[&str] = &["bit", "logic", "reg", "byte", "shortint", "int", "longint", "integer", "time", "enum"];
    BASES.iter().any(|b| t.strip_prefix(b).is_some_and(|rest| !rest.starts_with(|ch: char| ch.is_ascii_alphanumeric() || ch == '_')))
}

// ---- string dispatch ----

fn op_of<'t>(c: &Cst<'t>, n: NodeId) -> &'t str {
    c.toks(n).next().map(|t| c.text_of(t)).unwrap_or("")
}

/// `x == "lit"` pairs, also through `||`, where x is a string variable.
fn string_comparisons(d: &Design, ix: &Index, r: &Routine, e: NodeId, out: &mut Vec<(SymInfo, String)>) {
    let c = r.c;
    let x = c.strip(e);
    if c.kind(x) != BinaryExpr {
        return;
    }
    let kids: Vec<NodeId> = c.tree.children(x).collect();
    if kids.len() != 2 {
        return;
    }
    match op_of(c, x) {
        "||" => {
            string_comparisons(d, ix, r, kids[0], out);
            string_comparisons(d, ix, r, kids[1], out);
        }
        "==" | "===" => {
            let (mut lit, mut val) = (kids[1], kids[0]);
            if c.string_value(lit).is_none() {
                std::mem::swap(&mut lit, &mut val);
            }
            let Some(v) = c.string_value(lit) else { return };
            if c.string_value(val).is_some() {
                return;
            }
            if let Some(s) = referenced(d, ix, r, val).filter(|s| is_string_type(&ix.typedefs, &s.ty)) {
                out.push((s, v));
            }
        }
        _ => {}
    }
}

/// A string variable compared against literals in one case statement or
/// if/else-if or ?: chain: the variable, its literals and where the
/// statement is.
struct Dispatch {
    sym: SymInfo,
    lits: BTreeSet<String>,
    loc: Loc,
}

/// slang's StringDispatchVisitor over one routine body.
fn dispatches(d: &Design, ix: &Index, r: &Routine, args_only: bool, threshold: usize) -> Vec<Dispatch> {
    let c = r.c;
    let wanted = |s: &SymInfo| s.formal == args_only;
    let mut out = Vec::new();
    let mut in_chain: HashSet<NodeId> = HashSet::new();
    let chain = |conds: Vec<NodeId>, loc: Loc, out: &mut Vec<Dispatch>| {
        let mut order: Vec<SymInfo> = Vec::new();
        let mut lits: HashMap<Sym, BTreeSet<String>> = HashMap::new();
        for cond in conds {
            let mut pairs = Vec::new();
            string_comparisons(d, ix, r, cond, &mut pairs);
            for (s, lit) in pairs {
                if !wanted(&s) || lit.is_empty() {
                    continue;
                }
                if !lits.contains_key(&s.sym) {
                    order.push(s.clone());
                }
                lits.entry(s.sym.clone()).or_default().insert(lit);
            }
        }
        if let Some(s) = order.into_iter().find(|s| lits[&s.sym].len() >= threshold) {
            let l = lits.remove(&s.sym).unwrap_or_default();
            out.push(Dispatch { sym: s, lits: l, loc });
        }
    };
    for n in c.subtree(r.body) {
        match c.kind(n) {
            CaseStmt => {
                let parts = case_parts(c, n);
                let Some(e) = parts.expr else { continue };
                let Some(s) = referenced(d, ix, r, e).filter(|s| is_string_type(&ix.typedefs, &s.ty) && wanted(s)) else { continue };
                let mut lits: BTreeSet<String> =
                    parts.items.iter().flat_map(|(labels, ..)| labels.iter().filter_map(|&l| c.string_value(l))).collect();
                lits.remove("");
                if lits.len() >= threshold {
                    out.push(Dispatch { sym: s, lits, loc: c.loc_of(n) });
                }
            }
            IfStmt if !in_chain.contains(&n) => {
                let mut conds = Vec::new();
                let mut s = Some(n);
                while let Some(x) = s {
                    in_chain.insert(x);
                    let (cond, _, other) = if_parts(c, x);
                    conds.extend(cond);
                    s = other.filter(|&o| c.kind(o) == IfStmt);
                }
                chain(conds, c.loc_of(n), &mut out);
            }
            CondExpr if !in_chain.contains(&n) => {
                let mut conds = Vec::new();
                let mut e = Some(n);
                while let Some(x) = e {
                    in_chain.insert(x);
                    let kids: Vec<NodeId> = c.tree.children(x).collect();
                    conds.extend(kids.first().copied());
                    e = kids.get(2).map(|&f| c.strip(f)).filter(|&f| c.kind(f) == CondExpr);
                }
                chain(conds, c.loc_of(n), &mut out);
            }
            _ => {}
        }
    }
    out
}

fn literal_list(lits: &BTreeSet<String>) -> String {
    let mut out = String::new();
    for (n, l) in lits.iter().enumerate() {
        if n == 4 {
            out.push_str(", ...");
            break;
        }
        out.push_str(if out.is_empty() { "\"" } else { ", \"" });
        out.push_str(l);
        out.push('"');
    }
    out
}

/// UVM names register access policies with strings
/// (uvm_reg_field::get_access()), so code that dispatches on them has no
/// enum to use.
fn all_uvm_access_policies(lits: &BTreeSet<String>) -> bool {
    const POLICIES: &[&str] = &[
        "RO", "RW", "RC", "RS", "WRC", "WRS", "WC", "WS", "WSRC", "WCRS", "W1C", "W1S", "W1T", "W0C", "W0S", "W0T", "W1SRC",
        "W1CRS", "W0SRC", "W0CRS", "WO", "WOC", "WOS", "W1", "WO1", "NOACCESS",
    ];
    lits.iter().all(|l| POLICIES.contains(&l.as_str()))
}

/// Variables assigned the result of a UVM library call (s = f.get_access()).
fn from_library(d: &Design, ix: &Index, r: &Routine) -> HashSet<Sym> {
    let c = r.c;
    let mut out = HashSet::new();
    let Some(m) = r.m else { return out };
    let calls = Calls::new(m);
    for n in c.subtree(r.body) {
        if c.kind(n) != BinaryExpr || op_of(c, n) != "=" {
            continue;
        }
        let kids: Vec<NodeId> = c.tree.children(n).collect();
        let [lhs, rhs] = kids[..] else { continue };
        let rhs = c.strip(rhs);
        if c.kind(rhs) != CallExpr || c.callee_name(rhs).starts_with('$') {
            continue;
        }
        let owner = calls.owner(c, rhs);
        let lib = !owner.is_empty() && (owner.starts_with("uvm_") || d.classes().iter().any(|k| k.name == owner && is_library(d, k)));
        if !lib {
            continue;
        }
        if let Some(s) = referenced(d, ix, r, lhs) {
            out.insert(s.sym);
        }
    }
    out
}

fn string_as_enum(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for r in user_routines(d, &ix) {
        let lib = from_library(d, &ix, &r);
        for x in dispatches(d, &ix, &r, false, 3) {
            if lib.contains(&x.sym.sym) || all_uvm_access_policies(&x.lits) {
                continue;
            }
            out.add(
                "SV-CNS-004",
                "StringAsEnum",
                Severity::Warning,
                format!(
                    "String '{}' is compared against {} literals ({}) to choose what to do. The compiler cannot catch a misspelt or missing \
value. Declare an enum and convert the string once where it enters, for example with uvm_enum_wrapper#(T)::from_name().",
                    x.sym.name,
                    x.lits.len(),
                    literal_list(&x.lits)
                ),
                x.loc,
            );
        }
    }
}

fn string_mode_parameter(d: &Design, csts: &[Cst], out: &mut Sink) {
    let ix = Index::new(csts);
    for r in user_routines(d, &ix) {
        if let (Some(cls), Some(m)) = (r.cls, r.m) {
            if overrides_base(d, cls, &m.name) {
                continue;
            }
        }
        let name = match r.m {
            Some(m) => m.name.clone(),
            None => r.c.tree.child(r.decl, Name).map(|n| r.c.tree.bare_text(n)).unwrap_or_default(),
        };
        let mut reported = HashSet::new();
        for x in dispatches(d, &ix, &r, true, 2) {
            if all_uvm_access_policies(&x.lits) || !reported.insert(x.sym.sym.clone()) {
                continue;
            }
            out.add(
                "SV-CPL-001",
                "StringModeParameter",
                Severity::Warning,
                format!(
                    "'{name}' chooses its behaviour from the string argument '{}' ({}). Every caller has to spell these values exactly, \
and a typo is not caught. Take an enum argument, or split the routine into one routine per mode.",
                    x.sym.name,
                    literal_list(&x.lits)
                ),
                x.sym.loc,
            );
        }
    }
}

// ---- SV-CNS-005 ----

fn manual_bit_field_slicing(d: &Design, csts: &[Cst], out: &mut Sink) {
    const THRESHOLD: usize = 3;
    let ix = Index::new(csts);
    let mut uses: Vec<(SymInfo, BTreeSet<String>, Loc)> = Vec::new();
    let mut index: HashMap<Sym, usize> = HashMap::new();
    for r in user_routines(d, &ix) {
        let c = r.c;
        for n in c.subtree(r.body) {
            if c.kind(n) != SelectExpr || c.tok(n, ":").is_none() {
                continue;
            }
            let kids: Vec<NodeId> = c.tree.children(n).collect();
            let [v, left, right] = kids[..] else { continue };
            let vs = c.strip(v);
            if !matches!(c.kind(vs), NameExpr | MemberExpr) {
                continue;
            }
            let Some(s) = referenced(d, &ix, &r, vs) else { continue };
            if s.library || !is_integral_type(&ix.typedefs, &s.ty) {
                continue;
            }
            let i = *index.entry(s.sym.clone()).or_insert_with(|| {
                uses.push((s.clone(), BTreeSet::new(), c.loc_of(n)));
                uses.len() - 1
            });
            uses[i].1.insert(format!("{}:{}", c.expr_text(left), c.expr_text(right)));
        }
    }
    for (s, ranges, loc) in uses {
        if ranges.len() < THRESHOLD {
            continue;
        }
        let list: Vec<String> = ranges.iter().map(|r| format!("[{r}]")).collect();
        out.add(
            "SV-CNS-005",
            "ManualBitFieldSlicing",
            Severity::Info,
            format!(
                "'{}' is sliced into {} fixed fields ({}). The layout is spread over these numbers and breaks quietly when one changes. \
Declare a packed struct with named fields and use it (or cast to it) instead.",
                s.name,
                ranges.len(),
                list.join(", ")
            ),
            loc,
        );
    }
}
