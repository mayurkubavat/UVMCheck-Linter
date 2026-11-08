//! SV-DUP-005 (CopiedAgentPackage): Flags two packages whose classes have the
//! same roles, fields and method bodies apart from renamed identifiers, for at
//! least three classes and 80% of the classes in the larger package (classes
//! with almost no code are not counted).

use crate::support::class_roles::{class_methods, role, role_name, user_classes, Index};
use crate::support::written_tokens::{tok_where, token_key, written_tokens, Generated};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::{Class, Design, Loc};
use std::collections::{BTreeMap, HashMap};
use sv_syntax::SyntaxKind::*;

/// A class's shape: its role, property count and method bodies with
/// renaming; and how many tokens its methods have.
fn class_shape(d: &Design, ix: &Index, cls: &Class) -> (String, usize) {
    let mut bodies = Vec::new();
    let mut tokens = 0;
    for m in class_methods(cls) {
        let Some((c, n)) = ix.decl(m.loc) else { continue };
        if !matches!(c.kind(n), FunctionDecl | TaskDecl) {
            continue;
        }
        let mut toks = Vec::new();
        written_tokens(c, n, &mut toks);
        tokens += toks.len();
        bodies.push(token_key(&toks, true));
    }
    bodies.sort();
    let mut shape = format!("{}|{}", role_name(role(d, cls)), cls.properties.len());
    for b in bodies {
        shape.push('|');
        shape.push_str(&b);
    }
    (shape, tokens)
}

pub(crate) fn copied_agent_package(d: &Design, csts: &[Cst], out: &mut Sink) {
    // Option `min_classes` (rules.SV-DUP-005.options).
    fn min_classes() -> usize {
        crate::options::get("SV-DUP-005", "min_classes", 3) as usize
    }
    // Option `min_similarity` (rules.SV-DUP-005.options).
    fn min_percent() -> usize {
        crate::options::get("SV-DUP-005", "min_similarity", 80) as usize
    }
    // Option `min_class_tokens` (rules.SV-DUP-005.options).
    fn min_tokens() -> usize {
        crate::options::get("SV-DUP-005", "min_class_tokens", 12) as usize
    }
    let ix = Index::new(csts);
    let mut generated = Generated::new(d, csts);
    // Package name -> (where it is declared, class shapes).
    let mut order: Vec<(String, Loc, String)> = Vec::new();
    let mut shapes: HashMap<String, BTreeMap<String, usize>> = HashMap::new();
    let mut count: HashMap<String, usize> = HashMap::new();
    for cls in user_classes(d) {
        let Some((c, decl)) = ix.decl(cls.loc) else { continue };
        let Some(p) = c.parent(decl).filter(|&p| c.kind(p) == PackageDecl) else { continue };
        if generated.at(cls.loc) {
            continue;
        }
        let Some((_, pt)) = c.tree.child(p, Name).and_then(|n| c.tree.token_bounds(n)) else { continue };
        let pkg = c.text_of(pt).to_string();
        let (shape, tokens) = class_shape(d, &ix, cls);
        if tokens < min_tokens() {
            continue;
        }
        if !count.contains_key(&pkg) {
            order.push((pkg.clone(), c.loc(pt), tok_where(c, pt)));
        }
        *count.entry(pkg.clone()).or_default() += 1;
        *shapes.entry(pkg).or_default().entry(shape).or_default() += 1;
    }
    for i in 0..order.len() {
        for j in 0..i {
            let (a, _, aat) = &order[j];
            let (b, bloc, _) = &order[i];
            let (sa, sb) = (&shapes[a], &shapes[b]);
            let common: usize = sa.iter().map(|(k, n)| (*n).min(sb.get(k).copied().unwrap_or(0))).sum();
            let largest = count[a].max(count[b]);
            if common < min_classes() || common * 100 < min_percent() * largest {
                continue;
            }
            out.add(
                "SV-DUP-005",
                "CopiedAgentPackage",
                Severity::Info,
                format!(
                    "Package '{b}' matches '{a}' ({}) in {common} of {largest} classes, apart from names. It looks like a copied agent; \
make one parameterized agent, or a shared base with small subclasses, so a fix lands in both.",
                    aat
                ),
                *bloc,
            );
            break;
        }
    }
}
