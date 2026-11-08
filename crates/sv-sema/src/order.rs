//! Declaration before use (IEEE 1800 6.18, 23.8): a simple name used in
//! a design unit, package or the compilation unit must be declared before
//! the place it is used, in that scope or one that encloses it. A name
//! that only an enclosing scope declares later is "used before its
//! declaration".
//!
//! What is checked: values (variables, nets, parameters, enum values,
//! specparams, genvars) used in expressions, `.name` port connections
//! and modport ports, and typedef names used as types. Subroutines,
//! instances, classes and other definitions may be used before they are
//! declared.
//!
//! Kept lenient so that a legal design never gets an error: nothing in
//! a class, covergroup or constraint, in a `with` clause or an attribute,
//! in a port list (non-ANSI port expressions name later declarations),
//! in a method defined outside its class, or in a bind directive or
//! defparam is checked; any earlier declaration of the name in an
//! enclosing block counts, whatever it declares; a name some package
//! declares is not checked in a file that imports a package; and only
//! declarations in the same file count.

use std::collections::HashMap;

use sv_syntax::{Child, NodeId, SyntaxKind, Tree};
use SyntaxKind::*;

use crate::names::Diag;
use crate::scopes::{Scopes, SymKind};

/// Nodes whose declarations are local to them.
const CONTAINERS: &[SyntaxKind] = &[
    ModuleDecl,
    InterfaceDecl,
    ProgramDecl,
    PackageDecl,
    CheckerDecl,
    ClassDecl,
    FunctionDecl,
    TaskDecl,
    SeqBlock,
    ParBlock,
    GenerateBlock,
    LoopStmt,
    LoopGenerate,
    SequenceDecl,
    PropertyDecl,
    CovergroupDecl,
    ClockingDecl,
];

/// Uses inside these are not checked.
const SKIP: &[SyntaxKind] = &[
    ClassDecl,
    CovergroupDecl,
    ConstraintDecl,
    ConstraintBlock,
    WithClause,
    Attribute,
    AttrSpec,
    BindDirective,
    PortList,
];

#[derive(Clone, Copy)]
struct Decl {
    pos: u32,
    /// A value or typedef: a use before it is an error.
    checked: bool,
    typedef: bool,
}

/// Runs this module's checks over every tree of a compile and returns the
/// errors found (names used before they are declared), in tree and source order.
pub fn check(trees: &[Tree], scopes: &Scopes) -> Vec<Diag> {
    // Names some package declares.
    let mut in_package = std::collections::HashSet::new();
    for s in &scopes.symbols {
        let opener = scopes.scopes[s.scope].symbol.map(|y| scopes.symbols[y].kind);
        if opener == Some(SymKind::Package) {
            in_package.insert(s.name.as_str());
        }
    }
    let mut out = Vec::new();
    for (ix, t) in trees.iter().enumerate() {
        Order::new(ix, t, scopes, &in_package).run(&mut out);
    }
    out
}

struct Order<'a> {
    ix: usize,
    t: &'a Tree,
    parent: Vec<u32>,
    decls: HashMap<(NodeId, String), Vec<Decl>>,
    imports: bool,
    in_package: &'a std::collections::HashSet<&'a str>,
}

fn pos(t: &Tree, n: NodeId) -> Option<u32> {
    t.token_bounds(n).map(|(a, _)| a)
}

fn own_toks(t: &Tree, n: NodeId) -> Vec<&str> {
    t.node(n)
        .children
        .iter()
        .filter_map(|c| match c {
            Child::Token(i) => Some(&*t.tokens[*i as usize].text),
            Child::Node(_) => None,
        })
        .collect()
}

fn is_ident(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

impl<'a> Order<'a> {
    fn new(ix: usize, t: &'a Tree, scopes: &Scopes, in_package: &'a std::collections::HashSet<&'a str>) -> Self {
        let mut parent = vec![u32::MAX; t.nodes.len()];
        for n in t.preorder() {
            for c in t.children(n) {
                parent[c.0 as usize] = n.0;
            }
        }
        let mut o = Order { ix, t, parent, decls: HashMap::new(), imports: false, in_package };
        for s in scopes.symbols.iter().filter(|s| s.tree == ix) {
            let Some(p) = pos(t, s.node) else { continue };
            let checked = matches!(
                s.kind,
                SymKind::Variable
                    | SymKind::Net
                    | SymKind::Parameter
                    | SymKind::Typedef
                    | SymKind::EnumValue
                    | SymKind::Specparam
                    | SymKind::Genvar
            );
            let typedef = s.kind == SymKind::Typedef;
            let c = o.container(s.node);
            o.decls.entry((c, s.name.clone())).or_default().push(Decl { pos: p, checked, typedef });
        }
        for n in t.preorder() {
            let k = t.kind(n);
            if k == ImportDecl {
                o.imports = true;
            }
            // Any other declarator: counts as declared, never as an error.
            let (name, at) = match k {
                Declarator => match t.child(n, Name) {
                    Some(nm) => (t.bare_text(nm), n),
                    None => continue,
                },
                // Anything else a name declares (forward typedefs, genvars).
                Name => (t.bare_text(n), n),
                LoopStmt if own_toks(t, n).first() == Some(&"foreach") => {
                    for v in own_toks(t, n).into_iter().filter(|s| is_ident(s) && *s != "foreach") {
                        o.decls.entry((n, v.to_string())).or_default().push(Decl {
                            pos: 0,
                            checked: false,
                            typedef: false,
                        });
                    }
                    continue;
                }
                _ => continue,
            };
            let Some(p) = pos(t, at) else { continue };
            let c = o.container(at);
            o.decls.entry((c, name)).or_default().push(Decl { pos: p, checked: false, typedef: false });
        }
        o
    }

    fn up(&self, n: NodeId) -> Option<NodeId> {
        let p = self.parent[n.0 as usize];
        (p != u32::MAX).then_some(NodeId(p))
    }

    fn container(&self, n: NodeId) -> NodeId {
        let mut cur = self.up(n);
        while let Some(a) = cur {
            if CONTAINERS.contains(&self.t.kind(a)) {
                return a;
            }
            cur = self.up(a);
        }
        self.t.root
    }

    fn skipped(&self, n: NodeId) -> bool {
        let t = self.t;
        let mut cur = self.up(n);
        while let Some(a) = cur {
            let k = t.kind(a);
            if SKIP.contains(&k) {
                return true;
            }
            if k == ParamDecl && own_toks(t, a).first() == Some(&"defparam") {
                return true;
            }
            // A method defined outside its class.
            if matches!(k, FunctionDecl | TaskDecl) && t.child(a, Name).is_some_and(|nm| t.bare_text(nm).contains("::"))
            {
                return true;
            }
            cur = self.up(a);
        }
        false
    }

    fn run(&self, out: &mut Vec<Diag>) {
        let t = self.t;
        for n in t.preorder() {
            let (name, typedef) = match t.kind(n) {
                NameExpr => {
                    let Some((a, b)) = t.token_bounds(n) else { continue };
                    if a != b {
                        continue;
                    }
                    // A called subroutine.
                    if self.up(n).is_some_and(|p| t.kind(p) == CallExpr && t.children(p).next() == Some(n)) {
                        continue;
                    }
                    (t.tokens[a as usize].text.to_string(), false)
                }
                DataType => {
                    let Some((a, b)) = t.token_bounds(n) else { continue };
                    if a != b {
                        continue;
                    }
                    (t.tokens[a as usize].text.to_string(), true)
                }
                // `.name` connection.
                Arg => {
                    let tk = own_toks(t, n);
                    if tk.len() != 2
                        || tk[0] != "."
                        || !self.up(n).is_some_and(|p| self.up(p).is_some_and(|i| t.kind(i) == Instance))
                    {
                        continue;
                    }
                    (tk[1].to_string(), false)
                }
                // Modport ports.
                Declarator if self.up(n).is_some_and(|p| t.kind(p) == ModportDecl) => {
                    let names: Vec<NodeId> = t.children(n).filter(|&c| t.kind(c) == Name).skip(1).collect();
                    for nm in names {
                        self.lookup(nm, &t.bare_text(nm), false, out);
                    }
                    continue;
                }
                _ => continue,
            };
            self.lookup(n, &name, typedef, out);
        }
    }

    fn lookup(&self, n: NodeId, name: &str, typedef: bool, out: &mut Vec<Diag>) {
        if !is_ident(name) || name.contains('$') {
            return;
        }
        if self.imports && self.in_package.contains(name) {
            return;
        }
        let Some(at) = pos(self.t, n) else { return };
        if self.skipped(n) {
            return;
        }
        let mut later = false;
        let mut cur = Some(self.container(n));
        while let Some(c) = cur {
            if let Some(ds) = self.decls.get(&(c, name.to_string())) {
                if ds.iter().any(|d| d.pos < at) {
                    return;
                }
                later |= ds.iter().any(|d| d.checked && d.pos > at && (!typedef || d.typedef));
            }
            if c == self.t.root {
                break;
            }
            cur = Some(self.container(c));
        }
        if later {
            out.push(Diag {
                tree: self.ix,
                node: n,
                message: format!("identifier '{name}' used before its declaration"),
            });
        }
    }
}
