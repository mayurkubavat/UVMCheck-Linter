//! Name checks on the scopes (builder phase N1): what IEEE 1800 rejects
//! about declarations and names without types or elaboration.
//!
//! - a name declared twice in one scope (6.5, 6.18 ...);
//! - a forward typedef never defined in its scope (6.18);
//! - a typedef of a type name declared nowhere (6.18);
//! - an `extern constraint` with no body (18.5.1);
//! - a class method named `randomize`, `rand_mode` or `constraint_mode`
//!   (18.6.3, 18.8, 18.9: built in, not overridable);
//! - a `pure virtual` method in a class that is not virtual (8.21);
//! - `new` on a variable of a virtual or interface class (8.21, 8.26.5);
//! - a randsequence production that is used but not defined (18.17).
//!
//! Every check is local or looks names up across the whole design, so a
//! name it cannot see (a generate block, an inherited member) never makes
//! an error.

use std::collections::{HashMap, HashSet};

use sv_syntax::{Child, NodeId, SyntaxKind, Tree};
use SyntaxKind::*;

use crate::scopes::{Scopes, SymKind, ROOT, UNIT};

/// An error at a node's first token.
#[derive(Clone, Debug)]
pub struct Diag {
    pub tree: usize,
    pub node: NodeId,
    pub message: String,
}

fn toks(t: &Tree, n: NodeId) -> Vec<&str> {
    t.node(n)
        .children
        .iter()
        .filter_map(|c| match c {
            Child::Token(i) => Some(&*t.tokens[*i as usize].text),
            Child::Node(_) => None,
        })
        .collect()
}

fn name_of(t: &Tree, n: NodeId) -> String {
    t.child(n, Name).map(|c| t.bare_text(c)).unwrap_or_default()
}

/// Types every design can name without declaring them (package std).
const STD_TYPES: &[&str] = &["semaphore", "mailbox", "process", "weak_reference"];

const BUILTIN_METHODS: &[&str] = &["randomize", "rand_mode", "constraint_mode"];

pub fn check(trees: &[Tree], scopes: &Scopes) -> Vec<Diag> {
    let mut out = Vec::new();
    redeclarations(trees, scopes, &mut out);
    let mut types: HashSet<String> = STD_TYPES.iter().map(|s| s.to_string()).collect();
    // Classes by name: whether each declaration may be instantiated.
    let mut classes: HashMap<String, Vec<bool>> = HashMap::new();
    let mut constraint_bodies = HashSet::new();
    // Names a forward typedef promises, and where.
    let mut forwards: Vec<(usize, NodeId, String)> = Vec::new();
    for (ix, t) in trees.iter().enumerate() {
        for n in t.preorder() {
            match t.kind(n) {
                TypedefDecl | ForwardTypedef | ClassDecl | CovergroupDecl | InterfaceDecl | ModuleDecl
                | ProgramDecl | CheckerDecl | NetTypeDecl => {
                    if t.kind(n) == ForwardTypedef {
                        // `typedef class c;`: the name is the last identifier.
                        let name = name_of(t, n);
                        let name = if name.is_empty() {
                            toks(t, n).iter().rev().find(|x| is_ident(x)).map(|x| x.to_string()).unwrap_or_default()
                        } else {
                            name
                        };
                        forwards.push((ix, n, name));
                        continue;
                    }
                    types.insert(name_of(t, n));
                    if t.kind(n) == ClassDecl {
                        let tk = toks(t, n);
                        let concrete = !tk.contains(&"virtual") && !tk.contains(&"interface");
                        classes.entry(name_of(t, n)).or_default().push(concrete);
                    }
                }
                ParamDecl => {
                    for c in t.children(n) {
                        match t.kind(c) {
                            Name => {
                                types.insert(t.bare_text(c));
                            }
                            Declarator => {
                                types.insert(name_of(t, c));
                            }
                            _ => {}
                        }
                    }
                }
                ConstraintDecl => {
                    let name = name_of(t, n);
                    if let Some((_, c)) = name.rsplit_once("::") {
                        constraint_bodies.insert(c.to_string());
                    }
                }
                _ => {}
            }
        }
    }
    for (ix, n, name) in forwards {
        if name.is_empty() {
            continue;
        }
        if !types.contains(&name) {
            out.push(Diag { tree: ix, node: n, message: format!("forward typedef '{name}' is never defined") });
        }
        types.insert(name);
    }
    for (ix, t) in trees.iter().enumerate() {
        let parent = parents(t);
        for n in t.preorder() {
            match t.kind(n) {
                TypedefDecl => {
                    let Some(dt) = t.child(n, DataType) else {
                        continue;
                    };
                    let ty = t.bare_text(dt);
                    let one_token = t.token_bounds(dt).is_some_and(|(a, b)| a == b);
                    if one_token && is_ident(&ty) && !sv_syntax::is_builtin_type(&ty) && !types.contains(&ty) {
                        out.push(Diag { tree: ix, node: dt, message: format!("unknown type '{ty}'") });
                    }
                }
                ConstraintPrototype => {
                    let name = name_of(t, n);
                    if toks(t, n).contains(&"extern") && !constraint_bodies.contains(&name) {
                        out.push(Diag {
                            tree: ix,
                            node: n,
                            message: format!("extern constraint '{name}' has no body"),
                        });
                    }
                }
                ClassDecl => class_checks(t, ix, n, &mut out),
                RandSequenceStmt => productions(t, ix, n, &mut out),
                NewExpr => abstract_new(t, ix, n, &parent, &classes, &mut out),
                _ => {}
            }
        }
    }
    out
}

/// A class's `extends` clause (in its header), if any.
pub(crate) fn extends(t: &Tree, class: NodeId) -> Option<NodeId> {
    let header = t.child(class, Header);
    t.children(class).chain(header.into_iter().flat_map(|h| t.children(h))).find(|&c| {
        t.kind(c) == Extends && t.token_bounds(c).is_some_and(|(f, _)| &*t.tokens[f as usize].text == "extends")
    })
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

fn is_ident(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && s.chars().all(is_ident_char)
}

fn parents(t: &Tree) -> Vec<u32> {
    let mut parent = vec![u32::MAX; t.nodes.len()];
    for n in t.preorder() {
        for c in t.children(n) {
            parent[c.0 as usize] = n.0;
        }
    }
    parent
}

/// A name declared twice in one scope. A port and its net or variable
/// share a name; `$unit` is checked per file.
fn redeclarations(trees: &[Tree], s: &Scopes, out: &mut Vec<Diag>) {
    let mut seen: HashMap<(usize, usize, &str), Vec<usize>> = HashMap::new();
    for (i, sym) in s.symbols.iter().enumerate() {
        if sym.scope == ROOT || sym.name.is_empty() {
            continue;
        }
        let file = if sym.scope == UNIT { sym.tree } else { 0 };
        seen.entry((sym.scope, file, sym.name.as_str())).or_default().push(i);
    }
    let mut dups: Vec<usize> = Vec::new();
    for ids in seen.values() {
        if ids.len() < 2 {
            continue;
        }
        // A non-ANSI port list may name a port twice: `module m(a, a);`.
        let bare = |i: usize| {
            let sym = &s.symbols[i];
            trees[sym.tree].token_bounds(sym.node).is_some_and(|(a, b)| a == b)
        };
        let port_ids: Vec<usize> = ids.iter().copied().filter(|&i| s.symbols[i].kind == SymKind::Port).collect();
        let ids: Vec<usize> = if port_ids.len() > 1 && port_ids.iter().all(|&i| bare(i)) {
            ids.iter().copied().filter(|i| !port_ids[1..].contains(i)).collect()
        } else {
            ids.clone()
        };
        if ids.len() < 2 {
            continue;
        }
        // A function's `input i;` and its `logic [1:0] i;`.
        let ports = ids
            .iter()
            .filter(|&&i| {
                let sym = &s.symbols[i];
                sym.kind == SymKind::Port || (sym.kind == SymKind::Argument && sym.dtype.is_none())
            })
            .count();
        let internal = ids.iter().filter(|&&i| matches!(s.symbols[i].kind, SymKind::Net | SymKind::Variable)).count();
        if ids.len() == 2 && ports == 1 && internal == 1 {
            continue;
        }
        // Report every declaration after the first that is not the port's own symbol.
        let mut skip_internal = ports > 0;
        for &i in &ids[1..] {
            let k = s.symbols[i].kind;
            if skip_internal && matches!(k, SymKind::Net | SymKind::Variable) {
                skip_internal = false;
                continue;
            }
            dups.push(i);
        }
    }
    dups.sort();
    for i in dups {
        let sym = &s.symbols[i];
        out.push(Diag { tree: sym.tree, node: sym.node, message: format!("redefinition of '{}'", sym.name) });
    }
}

fn class_checks(t: &Tree, ix: usize, n: NodeId, out: &mut Vec<Diag>) {
    let tk = toks(t, n);
    let is_virtual = tk.contains(&"virtual") || tk.contains(&"interface");
    let cname = name_of(t, n);
    for c in t.children(n) {
        if !matches!(t.kind(c), FunctionDecl | TaskDecl | MethodPrototype) {
            continue;
        }
        let name = name_of(t, c);
        if BUILTIN_METHODS.contains(&name.as_str()) {
            out.push(Diag {
                tree: ix,
                node: c,
                message: format!("'{name}' is a built-in method and cannot be overridden"),
            });
        }
        if t.kind(c) == MethodPrototype && toks(t, c).contains(&"pure") && !is_virtual {
            out.push(Diag {
                tree: ix,
                node: c,
                message: format!("pure virtual method '{name}' in class '{cname}', which is not virtual"),
            });
        }
    }
}

/// Productions used by a randsequence but defined in none of its rules.
fn productions(t: &Tree, ix: usize, n: NodeId, out: &mut Vec<Diag>) {
    let defined: HashSet<String> = t.children(n).filter(|&c| t.kind(c) == Production).map(|p| name_of(t, p)).collect();
    let mut stack: Vec<NodeId> = t.children(n).filter(|&c| t.kind(c) == Production).collect();
    while let Some(x) = stack.pop() {
        for c in t.children(x) {
            match t.kind(c) {
                RsProdItem => {
                    let name = name_of(t, c);
                    if !name.is_empty() && !defined.contains(&name) {
                        out.push(Diag { tree: ix, node: c, message: format!("unknown production '{name}'") });
                    }
                }
                // Code blocks hold statements, not productions.
                RsCodeBlock => {}
                RandSequenceStmt => {}
                _ => stack.push(c),
            }
        }
    }
}

/// `v = new` and `T v = new` where every class named T is virtual or an
/// interface class.
fn abstract_new(
    t: &Tree,
    ix: usize,
    n: NodeId,
    parent: &[u32],
    classes: &HashMap<String, Vec<bool>>,
    out: &mut Vec<Diag>,
) {
    let up = |x: NodeId| (parent[x.0 as usize] != u32::MAX).then(|| NodeId(parent[x.0 as usize]));
    // Only a plain `new` or `new(...)`, not `T::new` or `new[n]`.
    let first = t.token_bounds(n).map(|(f, _)| &*t.tokens[f as usize].text);
    if first != Some("new") || toks(t, n).contains(&"[") {
        return;
    }
    let Some(p) = up(n) else { return };
    let var = match t.kind(p) {
        // `T v = new;`
        Declarator => name_of(t, p),
        // `v = new;`
        BinaryExpr => {
            let kids: Vec<NodeId> = t.children(p).collect();
            if kids.len() != 2 || kids[1] != n || !toks(t, p).contains(&"=") || t.kind(kids[0]) != NameExpr {
                return;
            }
            t.bare_text(kids[0])
        }
        _ => return,
    };
    // The nearest declaration of the variable, walking out through blocks.
    let mut at = p;
    let ty = loop {
        let Some(a) = up(at) else { return };
        if t.kind(a) == ClassDecl {
            // It may be an inherited member: not ours to judge.
            if extends(t, a).is_some() {
                return;
            }
        }
        let found = t.children(a).filter(|&c| t.kind(c) == DataDecl).find_map(|d| {
            t.children(d).filter(|&x| t.kind(x) == Declarator).find(|&x| name_of(t, x) == var).map(|_| d)
        });
        if let Some(d) = found {
            break t.child(d, DataType).map(|x| t.bare_text(x));
        }
        // Function arguments.
        if let Some(pl) = t.child(a, PortList) {
            if t.children(pl).any(|port| t.child(port, Declarator).is_some_and(|x| name_of(t, x) == var)) {
                return;
            }
        }
        at = a;
    };
    let Some(ty) = ty else { return };
    if let Some(decls) = classes.get(&ty) {
        if !decls.is_empty() && decls.iter().all(|c| !c) {
            out.push(Diag {
                tree: ix,
                node: n,
                message: format!("cannot instantiate '{ty}': it is an abstract or interface class"),
            });
        }
    }
}
