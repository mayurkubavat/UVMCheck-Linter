//! What a reference reaches: the semantic question rules about interfaces
//! ask ("is this a signal of a virtual interface?", "does it go through a
//! clocking block?"), answered from the declarations instead of from how
//! the handle happens to be named.
//!
//! [`reach`] follows a reference such as `cfg.bus.drv.data` from the
//! declaration of its first name through the declared types: a class
//! property (also inherited, also in another file), a `virtual` interface
//! handle, a typedef of one, an interface instance or port, a clocking
//! block, a modport. It answers only when every step resolves to exactly
//! one declaration; a call in the path, a type parameter, a class or
//! interface declared twice or not compiled all give `None`, and the rule
//! falls back to what it did before.

use std::sync::Arc;

use sv_sema::fx::{FxMap as HashMap, FxSet as HashSet};
use sv_syntax::SyntaxKind::*;
use sv_syntax::{Child, NodeId, Tree};

use crate::support::slang_ast::{ancestors, decl_name, enclosing_class, lookup};
use crate::syntax::Cst;

/// A declared type, as far as following references needs it.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Ty {
    /// `virtual [interface] name[.modport]`.
    Virtual(String),
    /// A named type: a class, an interface, a typedef, or one this compile
    /// does not declare.
    Named(String),
    /// A built-in type, a struct, an enum: no interface behind it.
    Value,
}

#[derive(Default)]
struct Unit {
    /// Clocking blocks declared in it.
    clockings: HashSet<String>,
    /// Its modports.
    modports: HashSet<String>,
    /// Variables, nets, ports and instances.
    members: HashMap<String, Ty>,
    /// An `interface` (not a module or program).
    interface: bool,
}

#[derive(Default)]
struct Class {
    base: Option<String>,
    props: HashMap<String, Ty>,
}

/// The compile's interfaces, modules, classes and typedefs by name. A name
/// declared more than once is in `twice` and resolves to nothing.
#[derive(Default)]
struct Model {
    units: HashMap<String, Unit>,
    classes: HashMap<String, Class>,
    typedefs: HashMap<String, Ty>,
    twice: HashSet<String>,
}

fn own_tokens(t: &Tree, n: NodeId) -> impl Iterator<Item = &str> {
    t.node(n).children.iter().filter_map(move |c| match c {
        Child::Token(i) => Some(&*t.tokens[*i as usize].text),
        Child::Node(_) => None,
    })
}

fn is_ident(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '\\')
        && !sv_syntax::is_builtin_type(s)
        && !matches!(s, "virtual" | "interface" | "struct" | "union" | "enum" | "packed" | "signed" | "unsigned")
}

/// The type a `DataType` node writes.
fn ty_of(t: &Tree, dt: NodeId) -> Ty {
    let toks: Vec<&str> = own_tokens(t, dt).collect();
    if toks.first() == Some(&"virtual") {
        return match t.child(dt, Name) {
            Some(n) => Ty::Virtual(t.bare_text(n)),
            None => Ty::Value,
        };
    }
    if toks.iter().any(|x| matches!(*x, "struct" | "union" | "enum")) {
        return Ty::Value;
    }
    // `pkg::name`: the last identifier names the type.
    match toks.iter().rev().find(|x| is_ident(x)) {
        Some(name) => Ty::Named(name.to_string()),
        None => Ty::Value,
    }
}

fn declared(t: &Tree, decl: NodeId, into: &mut HashMap<String, Ty>) {
    let ty = t.child(decl, DataType).map_or(Ty::Value, |dt| ty_of(t, dt));
    for d in t.children(decl).filter(|&d| t.kind(d) == Declarator) {
        if let Some(n) = t.child(d, Name) {
            into.entry(t.bare_text(n)).or_insert_with(|| ty.clone());
        }
    }
}

fn unit_of(t: &Tree, n: NodeId) -> Unit {
    let mut u = Unit { interface: t.kind(n) == InterfaceDecl, ..Unit::default() };
    for k in t.children(n) {
        match t.kind(k) {
            DataDecl => declared(t, k, &mut u.members),
            ClockingDecl => {
                if let Some(x) = t.child(k, Name) {
                    u.clockings.insert(t.bare_text(x));
                }
            }
            ModportDecl => {
                for d in t.children(k).filter(|&d| t.kind(d) == Declarator) {
                    if let Some(x) = t.child(d, Name) {
                        u.modports.insert(t.bare_text(x));
                    }
                }
            }
            Instantiation => {
                let Some(dt) = t.child(k, DataType) else { continue };
                let ty = ty_of(t, dt);
                for i in t.children(k).filter(|&i| t.kind(i) == Instance) {
                    if let Some(x) = t.child(i, Name) {
                        u.members.entry(t.bare_text(x)).or_insert_with(|| ty.clone());
                    }
                }
            }
            Header => {
                for pl in t.children(k).filter(|&p| t.kind(p) == PortList) {
                    for p in t.children(pl).filter(|&p| t.kind(p) == Port) {
                        declared(t, p, &mut u.members);
                    }
                }
            }
            _ => {}
        }
    }
    u
}

fn class_of(t: &Tree, n: NodeId) -> Class {
    let base = t
        .child(n, Header)
        .and_then(|h| t.child(h, Extends))
        .filter(|&e| own_tokens(t, e).next() == Some("extends"))
        .and_then(|e| t.child(e, DataType))
        .and_then(|dt| match ty_of(t, dt) {
            Ty::Named(b) => Some(b),
            _ => None,
        });
    let mut c = Class { base, ..Class::default() };
    for k in t.children(n).filter(|&k| t.kind(k) == DataDecl) {
        declared(t, k, &mut c.props);
    }
    c
}

fn build(trees: &[Tree]) -> Model {
    let mut m = Model::default();
    let mut seen: HashSet<String> = HashSet::default();
    for t in trees {
        for n in t.preorder() {
            let kind = t.kind(n);
            if !matches!(kind, InterfaceDecl | ModuleDecl | ProgramDecl | ClassDecl | TypedefDecl) {
                continue;
            }
            let Some(name) = t.child(n, Name).map(|x| t.bare_text(x)) else { continue };
            if !seen.insert(name.clone()) {
                m.twice.insert(name);
                continue;
            }
            match kind {
                ClassDecl => {
                    m.classes.insert(name, class_of(t, n));
                }
                TypedefDecl => {
                    if let Some(dt) = t.child(n, DataType) {
                        m.typedefs.insert(name, ty_of(t, dt));
                    }
                }
                _ => {
                    m.units.insert(name, unit_of(t, n));
                }
            }
        }
    }
    for name in &m.twice {
        m.units.remove(name);
        m.classes.remove(name);
        m.typedefs.remove(name);
    }
    m
}

fn model(c: &Cst) -> Arc<Model> {
    crate::shared::get("sem_model", c.compile_key(), || build(c.all))
}

/// Where following a reference has got to.
enum At {
    /// An interface (or module) by name; `virt`: through a virtual handle.
    Unit(String, bool),
    Class(String),
    /// Inside a clocking block.
    Clocking(bool),
    /// A value with no interface behind it.
    Value,
}

impl Model {
    fn at(&self, ty: &Ty, virt: bool) -> Option<At> {
        let mut ty = ty.clone();
        // Typedefs of typedefs: a short chain, or a cycle to give up on.
        for _ in 0..8 {
            match ty {
                Ty::Value => return Some(At::Value),
                Ty::Virtual(name) => return Some(At::Unit(name, true)),
                Ty::Named(name) => {
                    if let Some(next) = self.typedefs.get(&name) {
                        ty = next.clone();
                    } else if self.units.contains_key(&name) {
                        return Some(At::Unit(name, virt));
                    } else if self.classes.contains_key(&name) {
                        return Some(At::Class(name));
                    } else {
                        // A type parameter, a library class, a name declared twice.
                        return None;
                    }
                }
            }
        }
        None
    }

    /// The property `name` of class `class` or of a class it extends.
    fn prop(&self, class: &str, name: &str) -> Option<&Ty> {
        let mut cur = self.classes.get(class)?;
        for _ in 0..64 {
            if let Some(t) = cur.props.get(name) {
                return Some(t);
            }
            cur = self.classes.get(cur.base.as_deref()?)?;
        }
        None
    }
}

/// What a reference reaches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Reach {
    /// A signal of an interface, also through one of its clocking blocks.
    pub interface: bool,
    /// Through a clocking block.
    pub clocking: bool,
    /// Through a virtual interface handle.
    pub through_virtual: bool,
}

impl Reach {
    const VALUE: Reach = Reach { interface: false, clocking: false, through_virtual: false };

    /// A signal the testbench drives through a virtual interface or a
    /// clocking block: what the clocking-drive rules are about.
    pub(crate) fn driven_from_testbench(self) -> bool {
        self.interface && (self.through_virtual || self.clocking)
    }
}

/// The names of `a.b[i].c` from the left, or `None` for a reference with a
/// call or a scope operator in it.
fn segments(c: &Cst, e: NodeId, out: &mut Vec<String>) -> Option<()> {
    match c.kind(e) {
        NameExpr => out.push(c.tree.bare_text(e)),
        MemberExpr => {
            segments(c, c.tree.children(e).next()?, out)?;
            out.push(own_tokens(c.tree, e).filter(|x| *x != ".").last()?.to_string());
        }
        SelectExpr | ParenExpr => segments(c, c.tree.children(e).next()?, out)?,
        _ => return None,
    }
    Some(())
}

/// What the reference `e` (an assignment target, an operand) reaches, when
/// every name in it resolves; see the module documentation.
pub(crate) fn reach(c: &Cst, e: NodeId) -> Option<Reach> {
    let mut segs = Vec::new();
    segments(c, e, &mut segs)?;
    let m = model(c);
    let class_here = || enclosing_class(c, e).map(|k| decl_name(c, k)).filter(|n| !n.is_empty());
    let mut rest = segs.iter().map(String::as_str);
    let first = rest.next()?;
    let mut at = match first {
        "this" => At::Class(class_here().filter(|n| m.classes.contains_key(n))?),
        "super" => At::Class(m.classes.get(&class_here()?)?.base.clone().filter(|b| m.classes.contains_key(b))?),
        _ => match lookup(c, e, first) {
            Some(d) => m.at(&d.dtype.map_or(Ty::Value, |dt| ty_of(c.tree, dt)), false)?,
            None => root_elsewhere(c, e, &m, first, class_here())?,
        },
    };
    for seg in rest {
        at = match at {
            At::Value => return Some(Reach::VALUE),
            At::Clocking(virt) => return Some(Reach { interface: true, clocking: true, through_virtual: virt }),
            At::Class(k) => m.at(m.prop(&k, seg)?, false)?,
            At::Unit(name, virt) => {
                // `virtual x_if` with x_if not compiled: a signal, as far as anyone can tell.
                let Some(u) = m.units.get(&name) else {
                    return if m.twice.contains(&name) {
                        None
                    } else {
                        Some(Reach { interface: virt, clocking: false, through_virtual: virt })
                    };
                };
                if u.clockings.contains(seg) {
                    At::Clocking(virt)
                } else if u.modports.contains(seg) {
                    At::Unit(name, virt)
                } else {
                    match u.members.get(seg).map(|t| m.at(t, virt)) {
                        // A nested interface instance or interface port.
                        Some(Some(At::Unit(inner, v))) => At::Unit(inner, v),
                        Some(Some(At::Class(k))) => At::Class(k),
                        Some(_) => {
                            return Some(Reach { interface: u.interface, clocking: false, through_virtual: virt })
                        }
                        // Not a declaration of the unit: a task, a generate block.
                        None => return None,
                    }
                }
            }
        };
    }
    // The handle, the clocking block or the instance itself: not a signal.
    Some(match at {
        At::Clocking(virt) => Reach { interface: true, clocking: true, through_virtual: virt },
        _ => Reach::VALUE,
    })
}

/// A first name no block, argument list or scope of this file declares: an
/// inherited property of a class in another file, or a clocking block or an
/// instance of the design unit around.
fn root_elsewhere(c: &Cst, e: NodeId, m: &Model, name: &str, class: Option<String>) -> Option<At> {
    if let Some(k) = class {
        return m.at(m.prop(&k, name)?, false);
    }
    let unit = ancestors(c, e).find(|&a| matches!(c.kind(a), InterfaceDecl | ModuleDecl | ProgramDecl))?;
    let u = m.units.get(&decl_name(c, unit))?;
    if u.clockings.contains(name) {
        return Some(At::Clocking(false));
    }
    m.at(u.members.get(name)?, false)
}
