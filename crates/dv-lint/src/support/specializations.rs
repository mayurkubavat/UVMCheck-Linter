//! How many specializations of each parameterized class slang
//! elaborates. The engine's shared AST walk visits every specialization of
//! a generic class (the default one included), so findings in its body come
//! once per specialization.
//!
//! Specializations are found from the `C #(...)` references in the tree,
//! with arguments put in a canonical form (defaults filled in, typedefs
//! resolved) and references inside a generic class expanded for each of
//! its own specializations, until no new ones appear.

use std::collections::BTreeSet;
use sv_sema::fx::FxMap as HashMap;

use sv_syntax::{NodeId, SyntaxKind};

use crate::syntax::Cst;
use SyntaxKind::*;

struct Generic {
    node: NodeId,
    params: Vec<(String, Option<NodeId>)>,
    /// Typedefs declared directly in the class body.
    locals: HashMap<String, NodeId>,
}

/// Arguments of a reference: optional `.name`, value node.
type Args = Vec<(Option<String>, Option<NodeId>)>;

struct Ref {
    generic: usize,
    args: Args,
    /// The generic class whose body holds the reference.
    context: Option<usize>,
}

struct Engine<'a, 't> {
    c: &'a Cst<'t>,
    generics: Vec<Generic>,
    by_name: sv_sema::fx::FxMap<String, usize>,
    typedefs: HashMap<String, NodeId>,
}

const MAX_SPECS: usize = 256;

impl Engine<'_, '_> {
    /// Canonical text of a type or value node under a substitution.
    fn canon(&self, n: NodeId, subst: &HashMap<String, String>, ctx: Option<usize>, depth: usize) -> String {
        let c = self.c;
        if depth > 16 {
            return c.tree.bare_text(n);
        }
        // A reference to a generic class: canonical with its defaults.
        if let Some((g, args)) = self.as_ref(n) {
            if let Some(t) = self.tuple(g, &args, subst, ctx, depth + 1) {
                let name = self.name_of(g);
                return format!("{name}#({})", t.join(","));
            }
        }
        let toks = c.tree.token_range(n);
        if toks.len() == 1 {
            let t = c.text_of(toks[0]);
            if let Some(v) = subst.get(t) {
                return v.clone();
            }
            if let Some(&td) = ctx.and_then(|g| self.generics[g].locals.get(t)) {
                return self.canon(td, subst, ctx, depth + 1);
            }
            if let Some(&td) = self.typedefs.get(t) {
                return self.canon(td, &HashMap::default(), None, depth + 1);
            }
            return t.to_string();
        }
        let mut out = String::new();
        for &t in &toks {
            let s = c.text_of(t);
            out.push_str(subst.get(s).map(String::as_str).unwrap_or(s));
        }
        out
    }

    fn name_of(&self, g: usize) -> String {
        let c = self.c;
        c.tree.child(self.generics[g].node, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default()
    }

    /// `name #(args)` as a DataType or name expression.
    fn as_ref(&self, n: NodeId) -> Option<(usize, Args)> {
        let c = self.c;
        if !matches!(c.kind(n), DataType | NameExpr) {
            return None;
        }
        let first = c.toks(n).next()?;
        let &g = self.by_name.get(c.text_of(first))?;
        let pl = c.tree.child(n, ParamList);
        // Only the bare name and its parameters (not `pkg::C` forms).
        if c.toks(n).count() != 1 {
            return None;
        }
        let mut args = Vec::new();
        if let Some(pl) = pl {
            if let Some(al) = c.tree.child(pl, ArgList) {
                for a in c.tree.children(al).filter(|&x| c.kind(x) == Arg) {
                    let named = c.toks(a).any(|t| c.text_of(t) == ".");
                    let name = named
                        .then(|| {
                            c.toks(a).map(|t| c.text_of(t)).find(|x| !matches!(*x, "." | "(" | ")")).map(str::to_string)
                        })
                        .flatten();
                    args.push((name, c.tree.children(a).next()));
                }
            }
        }
        Some((g, args))
    }

    /// The full argument tuple of a specialization, or None when a
    /// parameter has no value.
    fn tuple(
        &self,
        g: usize,
        args: &[(Option<String>, Option<NodeId>)],
        subst: &HashMap<String, String>,
        ctx: Option<usize>,
        depth: usize,
    ) -> Option<Vec<String>> {
        let params = &self.generics[g].params;
        let mut own: HashMap<String, String> = HashMap::default();
        let mut out = Vec::new();
        for (ix, (pname, default)) in params.iter().enumerate() {
            let given = args
                .iter()
                .find(|(n, _)| n.as_deref() == Some(pname.as_str()))
                .or_else(|| args.get(ix).filter(|(n, _)| n.is_none()));
            let v = match given {
                Some((_, Some(node))) => self.canon(*node, subst, ctx, depth),
                _ => self.canon((*default)?, &own, Some(g), depth),
            };
            own.insert(pname.clone(), v.clone());
            out.push(v);
        }
        Some(out)
    }
}

fn generic_params(c: &Cst, cls: NodeId) -> Option<Vec<(String, Option<NodeId>)>> {
    let h = c.tree.child(cls, Header)?;
    let pl = c.tree.child(h, ParamList)?;
    let mut out = Vec::new();
    for p in c.tree.children(pl).filter(|&x| c.kind(x) == ParamDecl) {
        if c.toks(p).any(|t| c.text_of(t) == "localparam") {
            continue;
        }
        if let Some(d) = c.tree.child(p, Declarator) {
            // `int W = 8`
            let Some(nm) = c.tree.child(d, Name) else { continue };
            let default = c.tree.children(d).find(|&x| c.kind(x) != Name && c.kind(x) != Dim);
            out.push((c.tree.bare_text(nm), default));
        } else if let Some(nm) = c.tree.child(p, Name) {
            // `type T = int`
            let default = c.tree.children(p).find(|&x| x != nm);
            out.push((c.tree.bare_text(nm), default));
        }
    }
    Some(out)
}

/// Number of specializations slang walks for each generic class (by class
/// declaration node).
pub(crate) fn spec_counts(c: &Cst) -> HashMap<u32, usize> {
    specs(c).iter().map(|(&n, s)| (n, s.tuples.len())).collect()
}

/// The specializations of one generic class: its parameter names and, for
/// each specialization, the canonical value of every parameter.
pub(crate) struct ClassSpecs {
    pub(crate) params: Vec<String>,
    pub(crate) tuples: BTreeSet<Vec<String>>,
}

/// [`ClassSpecs`] of every generic class of the tree, by declaration node
/// (cached per tree).
pub(crate) fn specs(c: &Cst) -> std::sync::Arc<HashMap<u32, ClassSpecs>> {
    crate::shared::get("class_specs", c.tree_key(), || compute(c))
}

fn compute(c: &Cst) -> HashMap<u32, ClassSpecs> {
    let mut generics = Vec::new();
    let mut by_name = sv_sema::fx::FxMap::default();
    for n in c.nodes(ClassDecl) {
        let Some(params) = generic_params(c, n) else { continue };
        let name = c.tree.child(n, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default();
        let mut locals = HashMap::default();
        for t in c.tree.children(n).filter(|&x| c.kind(x) == TypedefDecl) {
            if let (Some(nm), Some(ty)) = (c.tree.child(t, Name), c.tree.child(t, DataType)) {
                locals.insert(c.tree.bare_text(nm), ty);
            }
        }
        by_name.entry(name).or_insert(generics.len());
        generics.push(Generic { node: n, params, locals });
    }
    if generics.is_empty() {
        return HashMap::default();
    }
    let mut typedefs = HashMap::default();
    for t in c.nodes(TypedefDecl) {
        // Only typedefs outside classes are global.
        if c.parent(t).is_some_and(|p| c.kind(p) == ClassDecl) {
            continue;
        }
        if let (Some(nm), Some(ty)) = (c.tree.child(t, Name), c.tree.child(t, DataType)) {
            typedefs.insert(c.tree.bare_text(nm), ty);
        }
    }
    let engine = Engine { c, generics, by_name, typedefs };
    let class_of: HashMap<u32, usize> = engine.generics.iter().enumerate().map(|(i, g)| (g.node.0, i)).collect();
    // Out-of-block bodies of generic classes.
    let mut out_of_block: HashMap<u32, usize> = HashMap::default();
    for d in c.nodes(FunctionDecl).into_iter().chain(c.nodes(TaskDecl)) {
        let Some(nm) = c.tree.child(d, Name) else { continue };
        let toks: Vec<u32> = c.toks(nm).collect();
        if toks.len() >= 3 && c.text_of(toks[1]) == "::" {
            if let Some(&g) = engine.by_name.get(c.text_of(toks[0])) {
                out_of_block.insert(d.0, g);
            }
        }
    }
    let context = |n: NodeId| -> Option<usize> {
        let mut x = c.parent(n);
        while let Some(y) = x {
            if let Some(&g) = class_of.get(&y.0) {
                return Some(g);
            }
            if let Some(&g) = out_of_block.get(&y.0) {
                return Some(g);
            }
            x = c.parent(y);
        }
        None
    };
    let mut refs: Vec<Ref> = Vec::new();
    let pre: Vec<NodeId> = c.nodes(DataType).into_iter().chain(c.nodes(NameExpr)).collect();
    for n in pre {
        // The class's own header names are not references.
        if c.parent(n).is_some_and(|p| c.kind(p) == ParamDecl) && c.tree.child(n, ParamList).is_none() {
            continue;
        }
        let Some((g, args)) = engine.as_ref(n) else { continue };
        if c.kind(n) == NameExpr && c.tree.child(n, ParamList).is_none() {
            continue;
        }
        refs.push(Ref { generic: g, args, context: context(n) });
    }
    let mut specs: Vec<BTreeSet<Vec<String>>> = vec![BTreeSet::new(); engine.generics.len()];
    let mut queue: Vec<(usize, Vec<String>)> = Vec::new();
    let add =
        |g: usize, t: Vec<String>, specs: &mut Vec<BTreeSet<Vec<String>>>, queue: &mut Vec<(usize, Vec<String>)>| {
            if specs[g].len() < MAX_SPECS && specs[g].insert(t.clone()) {
                queue.push((g, t));
            }
        };
    let empty = HashMap::default();
    for g in 0..engine.generics.len() {
        // The default specialization, always built: a body that uses a
        // built-in default as a scope (`T::type_id`) is dropped from it
        // alone (crate::support::default_spec).
        if let Some(t) = engine.tuple(g, &[], &empty, None, 0) {
            add(g, t, &mut specs, &mut queue);
        }
    }
    for r in refs.iter().filter(|r| r.context.is_none()) {
        if let Some(t) = engine.tuple(r.generic, &r.args, &empty, None, 0) {
            add(r.generic, t, &mut specs, &mut queue);
        }
    }
    let mut steps = 0;
    while let Some((g, t)) = queue.pop() {
        steps += 1;
        if steps > 20_000 {
            break;
        }
        let subst: HashMap<String, String> =
            engine.generics[g].params.iter().map(|(n, _)| n.clone()).zip(t.iter().cloned()).collect();
        for r in refs.iter().filter(|r| r.context == Some(g)) {
            if let Some(nt) = engine.tuple(r.generic, &r.args, &subst, Some(g), 0) {
                add(r.generic, nt, &mut specs, &mut queue);
            }
        }
    }
    engine
        .generics
        .iter()
        .zip(specs)
        .map(|(g, tuples)| (g.node.0, ClassSpecs { params: g.params.iter().map(|p| p.0.clone()).collect(), tuples }))
        .collect()
}
