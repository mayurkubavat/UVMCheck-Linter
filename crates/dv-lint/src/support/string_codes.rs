//! Strings compared against literals and the dispatches they drive, for the
//! rules on values used as codes.

use std::collections::{BTreeSet, HashMap, HashSet};

use dv_model::{Class, ClassId, Design, Loc};

use svlang::syntax::NodeId;

use crate::support::routine_bodies::{bodies, case_parts, if_parts};

use crate::support::free_routines::free_routines;

use crate::support::class_roles::{class_of_type_in, is_library, Ctx, Index, Typedefs};

use crate::syntax::Cst;

use svlang::syntax::SyntaxKind::*;

/// A variable a name refers to: a routine's local or argument, or a class
/// property (by the class that declares it).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Sym {
    Local(usize, String),
    Prop(ClassId, String),
}

#[derive(Clone)]
pub(crate) struct SymInfo {
    pub(crate) sym: Sym,
    pub(crate) name: String,
    pub(crate) ty: String,
    pub(crate) formal: bool,
    pub(crate) library: bool,
    pub(crate) loc: Loc,
}

/// One user routine: its tree, declaration, body and (for methods) class
/// and model method.
pub(crate) struct Routine<'a, 'd, 't> {
    pub(crate) id: usize,
    pub(crate) c: &'a Cst<'t>,
    pub(crate) decl: NodeId,
    pub(crate) body: NodeId,
    pub(crate) cls: Option<&'d Class>,
    pub(crate) m: Option<&'d dv_model::Method>,
    /// Names it declares: name -> (type, is an argument, where).
    pub(crate) names: HashMap<String, (String, bool, Loc)>,
}

pub(crate) fn routine_names(c: &Cst, r: NodeId) -> HashMap<String, (String, bool, Loc)> {
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

pub(crate) fn user_routines<'a, 'd, 't>(d: &'d Design, ix: &Index<'a, 't>) -> Vec<Routine<'a, 'd, 't>> {
    let mut out = Vec::new();
    for (cls, m, c, body) in bodies(d, ix) {
        let Some(decl) = c.parent(body) else { continue };
        out.push(Routine { id: out.len(), c, decl, body, cls: Some(cls), m: Some(m), names: routine_names(c, decl) });
    }
    for c in ix.csts {
        for decl in free_routines(d, c) {
            // Out-of-block method bodies are the class's.
            if crate::support::routine_owner::in_library(d, c, decl)
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
pub(crate) fn find_prop<'d>(d: &'d Design, k: &'d Class, name: &str) -> Option<(&'d Class, &'d dv_model::Property)> {
    std::iter::once(k)
        .chain(d.ancestors(k.id))
        .find_map(|x| x.properties.iter().find(|p| p.name == name).map(|p| (x, p)))
}

pub(crate) fn typedef_base(td: &Typedefs, ty: &str) -> String {
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
pub(crate) fn referenced(d: &Design, ix: &Index, r: &Routine, e: NodeId) -> Option<SymInfo> {
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
            Some(SymInfo {
                sym: Sym::Prop(k.id, name.clone()),
                name,
                ty: p.ty.clone(),
                formal: false,
                library: is_library(d, k),
                loc: p.loc,
            })
        }
        MemberExpr => {
            let obj = c.tree.children(e).next()?;
            let member = c.last_text(e).to_string();
            let o = c.strip(obj);
            let oc = if c.kind(o) == NameExpr && matches!(c.tree.bare_text(o).as_str(), "this" | "super") {
                let k = r.cls?;
                if c.tree.bare_text(o) == "super" {
                    k.base.map(|b| d.class(b))?
                } else {
                    k
                }
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

pub(crate) fn is_string_type(td: &Typedefs, ty: &str) -> bool {
    typedef_base(td, ty) == "string"
}

pub(crate) fn op_of<'t>(c: &Cst<'t>, n: NodeId) -> &'t str {
    c.toks(n).next().map(|t| c.text_of(t)).unwrap_or("")
}

/// `x == "lit"` pairs, also through `||`, where x is a string variable.
pub(crate) fn string_comparisons(d: &Design, ix: &Index, r: &Routine, e: NodeId, out: &mut Vec<(SymInfo, String)>) {
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
pub(crate) struct Dispatch {
    pub(crate) sym: SymInfo,
    pub(crate) lits: BTreeSet<String>,
    pub(crate) loc: Loc,
}

/// slang's StringDispatchVisitor over one routine body.
pub(crate) fn dispatches(d: &Design, ix: &Index, r: &Routine, args_only: bool, threshold: usize) -> Vec<Dispatch> {
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
                let Some(s) = referenced(d, ix, r, e).filter(|s| is_string_type(&ix.typedefs, &s.ty) && wanted(s))
                else {
                    continue;
                };
                let mut lits: BTreeSet<String> = parts
                    .items
                    .iter()
                    .flat_map(|(labels, ..)| labels.iter().filter_map(|&l| c.string_value(l)))
                    .collect();
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

pub(crate) fn literal_list(lits: &BTreeSet<String>) -> String {
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
pub(crate) fn all_uvm_access_policies(lits: &BTreeSet<String>) -> bool {
    const POLICIES: &[&str] = &[
        "RO", "RW", "RC", "RS", "WRC", "WRS", "WC", "WS", "WSRC", "WCRS", "W1C", "W1S", "W1T", "W0C", "W0S", "W0T",
        "W1SRC", "W1CRS", "W0SRC", "W0CRS", "WO", "WOC", "WOS", "W1", "WO1", "NOACCESS",
    ];
    lits.iter().all(|l| POLICIES.contains(&l.as_str()))
}
