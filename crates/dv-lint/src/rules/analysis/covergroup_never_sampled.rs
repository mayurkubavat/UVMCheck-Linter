//! SV-COV-019 (CovergroupNeverSampled): Flags a covergroup declared in a
//! module, interface, program or package that never collects a sample: it
//! has no sampling event (`covergroup cg @(posedge clk)`), and either nothing
//! instantiates it, or no instance of it is ever sampled with `.sample()`
//! anywhere in the design.
//!
//! SV-COV-001 judges covergroups declared in classes, one file at a time;
//! this rule covers the others, across every file of the compile (an
//! interface's covergroup is often sampled from a bound checker or a test in
//! another file). Instances are the variables declared with the covergroup's
//! type, arrays included; a `.sample()` call counts for an instance when the
//! name it is called on (`cg_i.sample()`, `env.cov.cg_i.sample()`,
//! `cg_q[i].sample(x)`) is one of them.

use std::collections::{BTreeMap, HashSet};

use crate::support::slang_ast::{ancestors, kids};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use dv_model::{Design, Loc};
use svlang::syntax::NodeId;
use svlang::syntax::SyntaxKind::*;

/// The name a `.sample()` call is made on: `a.b[i].cg.sample()` -> `cg`.
fn sampled_name(c: &Cst, call: NodeId) -> Option<String> {
    let callee = *kids(c, call).first()?;
    if c.kind(callee) != MemberExpr || c.last_text(callee) != "sample" {
        return None;
    }
    let mut base = *kids(c, callee).first()?;
    while c.kind(base) == SelectExpr {
        base = *kids(c, base).first()?;
    }
    match c.kind(base) {
        NameExpr => Some(c.tree.bare_text(base)),
        MemberExpr => Some(c.last_text(base).to_string()),
        _ => None,
    }
}

pub(crate) fn covergroup_never_sampled(_: &Design, csts: &[Cst], out: &mut Sink) {
    // Covergroup name -> (declaration, without a sampling event), outside classes.
    let mut groups: BTreeMap<String, Vec<(Loc, bool, String)>> = BTreeMap::new();
    // Covergroup type -> its instance names.
    let mut instances: BTreeMap<String, HashSet<String>> = BTreeMap::new();
    let mut sampled: HashSet<String> = HashSet::new();
    let mut names_in_classes: HashSet<String> = HashSet::new();
    for c in csts {
        for g in c.nodes(CovergroupDecl) {
            let Some(nm) = c.tree.child(g, Name) else { continue };
            let name = c.tree.bare_text(nm);
            let Some(scope) = ancestors(c, g)
                .find(|&a| matches!(c.kind(a), ClassDecl | ModuleDecl | InterfaceDecl | ProgramDecl | PackageDecl))
            else {
                continue;
            };
            if c.kind(scope) == ClassDecl {
                names_in_classes.insert(name);
                continue;
            }
            let Some(t) = c.first_tok(nm) else { continue };
            let event = c.tree.child(g, TimingControl).is_some() || c.tree.child(g, BlockEvent).is_some();
            let owner = c.tree.child(scope, Name).map(|x| c.tree.bare_text(x)).unwrap_or_default();
            groups.entry(name).or_default().push((c.loc(t), event, owner));
        }
        for d in c.nodes(DataDecl) {
            let Some(ty) = c.tree.child(d, DataType).and_then(|t| c.first_tok(t)).map(|t| c.text_of(t)) else {
                continue;
            };
            for x in c.tree.children(d).filter(|&x| c.kind(x) == Declarator) {
                if let Some(n) = c.tree.child(x, Name) {
                    instances.entry(ty.to_string()).or_default().insert(c.tree.bare_text(n));
                }
            }
        }
        for call in c.nodes(CallExpr) {
            sampled.extend(sampled_name(c, call));
        }
    }
    for (name, decls) in groups {
        // A name a class also declares a covergroup with: instances and calls
        // cannot be told apart by name.
        if names_in_classes.contains(&name) || decls.len() != 1 {
            continue;
        }
        let (loc, event, owner) = &decls[0];
        if *event {
            continue;
        }
        let inst = instances.get(&name);
        let msg = match inst {
            None => format!(
                "Covergroup '{name}' in '{owner}' is never instantiated: nothing declares a variable of its type. It collects no \
                 coverage, and the report shows no group for it at all."
            ),
            Some(set) if !set.iter().any(|i| sampled.contains(i)) => {
                let mut v: Vec<&String> = set.iter().collect();
                v.sort();
                let names: Vec<&str> = v.iter().map(|s| s.as_str()).collect();
                format!(
                    "Covergroup '{name}' in '{owner}' has no sampling event and its instance(s) {} are never sampled with \
                     '.sample()'. It stays at 0% coverage.",
                    names.join(", ")
                )
            }
            Some(_) => continue,
        };
        out.add("SV-COV-019", "CovergroupNeverSampled", Severity::Warning, msg, *loc);
    }
}
