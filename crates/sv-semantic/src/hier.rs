//! Hierarchical names (23.6, 23.8) that name nothing: a dotted name in a
//! design unit whose first name is an instance of that unit (or a
//! top-level module) goes down the elaborated hierarchy; each next name
//! must be an instance or generate block there, or something the
//! definition reached declares.
//!
//! Lenient where the hierarchy cannot tell, so that a legal design never
//! gets an error:
//!
//! - only names in a design unit's own items (not in a generate block, a
//!   class, a `bind` directive or a defparam), and only paths that start
//!   at an instance below or at a top;
//! - a name selected with `[...]` on the way stops the walk;
//! - once the walk leaves the hierarchy at an instance of a definition,
//!   any identifier in that definition's text counts as declared there
//!   (signals, generate block labels, ports, modports); a generate block
//!   reached stops the walk, as do instances of unknown definitions.

use crate::fx::{FxMap as HashMap, FxSet as HashSet};

use sv_syntax::{Child, NodeId, SyntaxKind, Tree};
use SyntaxKind::*;

use crate::names::Diag;

/// One node of the elaborated hierarchy: an instance of a definition, or
/// a generate block (`None`).
type Hier = HashMap<String, Option<String>>;

/// The errors of `trees`' hierarchical names, given the elaborated
/// hierarchy (the lines of [`crate::Elaborated::hierarchy`]).
pub fn check(trees: &[Tree], hierarchy: &[String]) -> Vec<Diag> {
    let mut nodes: Hier = HashMap::default();
    let mut tops: HashSet<&str> = HashSet::default();
    let mut paths_of: HashMap<&str, Vec<String>> = HashMap::default();
    for l in hierarchy {
        let mut w = l.split(' ');
        let (Some(at), Some(kind), Some(name)) = (w.next(), w.next(), w.next()) else { continue };
        let full = if at == "$root" { name.to_string() } else { format!("{at}.{name}") };
        match kind {
            "instance" => {
                let Some(def) = l.rsplit(" : ").next() else { continue };
                if at == "$root" {
                    tops.insert(name);
                }
                paths_of.entry(def).or_default().push(full.clone());
                nodes.insert(full, Some(def.to_string()));
            }
            "generate" => {
                nodes.insert(full, None);
            }
            _ => {}
        }
    }
    if nodes.is_empty() {
        return Vec::new();
    }
    // Definitions by name (the first, as elaboration takes it).
    let mut defs: HashMap<String, (usize, NodeId)> = HashMap::default();
    for (ix, t) in trees.iter().enumerate() {
        for n in t.preorder() {
            if matches!(t.kind(n), ModuleDecl | InterfaceDecl | ProgramDecl) {
                if let Some(nm) = t.child(n, Name) {
                    defs.entry(t.bare_text(nm)).or_insert((ix, n));
                }
            }
        }
    }
    let mut idents: HashMap<String, HashSet<&str>> = HashMap::default();
    let mut declared = |def: &str, name: &str| -> Option<bool> {
        if !idents.contains_key(def) {
            let &(ix, n) = defs.get(def)?;
            let t = &trees[ix];
            let set = t.token_range(n).into_iter().map(|k| &*t.tokens[k as usize].text).collect();
            idents.insert(def.to_string(), set);
        }
        Some(idents[def].contains(name))
    };
    let mut out = Vec::new();
    for (ix, t) in trees.iter().enumerate() {
        let mut parent = vec![u32::MAX; t.nodes.len()];
        for n in t.preorder() {
            for c in t.children(n) {
                parent[c.0 as usize] = n.0;
            }
        }
        let up = |n: NodeId| (parent[n.0 as usize] != u32::MAX).then(|| NodeId(parent[n.0 as usize]));
        for n in t.preorder() {
            if t.kind(n) != MemberExpr {
                continue;
            }
            let Some(base) = t.children(n).next().filter(|&b| t.kind(b) == NameExpr) else { continue };
            let Some((a, b)) = t.token_bounds(base) else { continue };
            let first = &*t.tokens[a as usize].text;
            if a != b || !first.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
                continue;
            }
            // The names after the first, each with its select.
            let mut chain: Vec<(&str, NodeId)> = Vec::new();
            let mut cur = n;
            loop {
                let own: Vec<&str> = t
                    .node(cur)
                    .children
                    .iter()
                    .filter_map(|c| match c {
                        Child::Token(i) => Some(&*t.tokens[*i as usize].text),
                        Child::Node(_) => None,
                    })
                    .collect();
                let [".", m] = own[..] else { break };
                chain.push((m, cur));
                match up(cur) {
                    Some(p) if t.kind(p) == MemberExpr && t.children(p).next() == Some(cur) => cur = p,
                    _ => break,
                }
            }
            if chain.is_empty() {
                continue;
            }
            // The design unit it is in, through its own items only.
            let mut def = None;
            let mut at = up(n);
            while let Some(x) = at {
                match t.kind(x) {
                    ModuleDecl | InterfaceDecl | ProgramDecl => {
                        def = t.child(x, Name).map(|nm| t.bare_text(nm));
                        break;
                    }
                    GenerateRegion | GenerateBlock | LoopGenerate | IfGenerate | CaseGenerate | ClassDecl
                    | BindDirective => break,
                    _ if t.token_bounds(x).is_some_and(|(s, _)| &*t.tokens[s as usize].text == "defparam") => break,
                    _ => {}
                }
                at = up(x);
            }
            let Some(def) = def else { continue };
            let Some(paths) = paths_of.get(def.as_str()) else { continue };
            'paths: for p in paths {
                let below = format!("{p}.{first}");
                let mut cur = if nodes.contains_key(&below) {
                    below
                } else if tops.contains(first) {
                    first.to_string()
                } else {
                    continue;
                };
                for &(m, node) in &chain {
                    let next = format!("{cur}.{m}");
                    if nodes.contains_key(&next) {
                        cur = next;
                        continue;
                    }
                    let Some(Some(d)) = nodes.get(&cur) else { break };
                    let generated = m.strip_prefix("genblk").is_some_and(|r| r.bytes().all(|c| c.is_ascii_digit()));
                    if !generated && declared(d, m) == Some(false) {
                        out.push(Diag {
                            tree: ix,
                            node,
                            message: format!("could not resolve hierarchical path name '{m}'"),
                        });
                        break 'paths;
                    }
                    break;
                }
            }
        }
    }
    out
}
